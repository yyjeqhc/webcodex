use super::*;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use salvo::test::{RequestBuilder, ResponseExt, TestClient};
use serde_json::Value;
use sha2::{Digest, Sha256};

const ORIGIN: &str = "https://fixture.trycloudflare.com";
const CALLBACK: &str = "https://oauth-redirect.googleusercontent.com/r/test-app";

fn public_service(control: &Arc<CloudflareControl>) -> Service {
    Service::new(CloudflareOwner::router(
        control.clone(),
        Arc::new(crate::tool_runtime::ToolRuntime::new_for_tests()),
        Arc::new(crate::oauth_http::AuthorizeSessionStore::new()),
        Arc::new(crate::server_shutdown::ShutdownCoordinator::default()),
    ))
}

fn enable_dcr(fixture: &Fixture) {
    let path = fixture.control.root.join("tunnel.json");
    let mut profiles: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    profiles[0]["oauth_dynamic_registration_enabled"] = true.into();
    private_file(&path, &serde_json::to_vec(&profiles).unwrap());
}

fn form(pairs: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs.iter().copied())
        .finish()
}

fn post_form(url: &str, body: String) -> RequestBuilder {
    TestClient::post(url)
        .add_header("content-type", "application/x-www-form-urlencoded", true)
        .body(body)
}

async fn json_response(request: RequestBuilder, service: &Service, status: StatusCode) -> Value {
    let mut res = request.send(service).await;
    assert_eq!(res.status_code, Some(status));
    res.take_json().await.unwrap()
}

fn mcp(access: &str, method: &str, params: Value) -> RequestBuilder {
    TestClient::post(&format!("{ORIGIN}/mcp"))
        .bearer_auth(access)
        .add_header("user-agent", "Google", true)
        .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
}

#[tokio::test]
async fn discovery_recovers_after_a_failed_read_and_discards_stale_results() {
    let fixture = Fixture::new();
    let control = &fixture.control;
    enable_dcr(&fixture);
    let (prepared, entry) = activate_quick_fixture(control, ORIGIN);
    let now = Instant::now();
    // Schedule without the public network, then deliver each metadata result
    // through the same completion path used by the owner.
    let schedule = |time| {
        let mut guard = control.attempt.lock().unwrap();
        let attempt = guard.as_mut().unwrap();
        attempt.next_discovery_probe(time)?;
        attempt.discovery_task = Some(OwnedTask(tokio::spawn(async {})));
        Some(attempt.discovery_next_check)
    };
    let first = schedule(now).unwrap();
    control.record_discovery(&entry, first, false);
    assert_eq!(
        control.status("quick").unwrap().reason_code.as_deref(),
        Some("cloudflare_discovery_unavailable")
    );
    control
        .attempt
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .discovery_task
        .take();
    assert!(schedule(now).is_none());
    let retry = schedule(first).unwrap();
    control.record_discovery(&entry, retry, true);
    assert!(control.status("quick").unwrap().registration_available);
    {
        let mut guard = control.attempt.lock().unwrap();
        let attempt = guard.as_mut().unwrap();
        attempt.observe_forwarding(false);
        attempt.observe_forwarding(true);
    }
    control.record_discovery(&entry, retry, true);
    assert_eq!(
        control.status("quick").unwrap().reason_code.as_deref(),
        Some("cloudflare_discovery_pending")
    );
    let reconnect = schedule(retry).unwrap();
    control.record_discovery(&entry, retry, true);
    assert!(!control.status("quick").unwrap().registration_available);
    control.record_discovery(&entry, reconnect, true);
    assert!(control.status("quick").unwrap().registration_available);

    control
        .stop(
            "quick",
            &control.instance_id,
            prepared.process_generation,
            None,
        )
        .unwrap();
    activate_quick_fixture(control, ORIGIN);
    let current = schedule(retry).unwrap();
    control.record_discovery(&entry, current, true);
    assert!(!control.status("quick").unwrap().registration_available);
}

#[tokio::test]
async fn cloudflare_dcr_metadata_and_registration_require_admission_policy_owner_and_authority() {
    let fixture = Fixture::new();
    let db = &fixture.control.db;
    let service = public_service(&fixture.control);
    let discovery = || TestClient::get(&format!("{ORIGIN}/.well-known/oauth-authorization-server"));
    let register = || {
        TestClient::post(&format!("{ORIGIN}/oauth/register"))
            .json(&json!({"redirect_uris":[CALLBACK]}))
    };
    assert_eq!(
        discovery().send(&service).await.status_code,
        Some(StatusCode::SERVICE_UNAVAILABLE)
    );
    let (prepared, _) = activate_quick_fixture(&fixture.control, ORIGIN);
    assert!(
        !fixture
            .control
            .status("quick")
            .unwrap()
            .registration_available
    );
    let metadata = json_response(discovery(), &service, StatusCode::OK).await;
    assert!(metadata.get("registration_endpoint").is_none());
    assert_eq!(
        register().send(&service).await.status_code,
        Some(StatusCode::NOT_FOUND)
    );
    fixture
        .control
        .stop(
            "quick",
            &prepared.server_instance_id,
            prepared.process_generation,
            None,
        )
        .unwrap();
    enable_dcr(&fixture);
    let (_, entry) = activate_quick_fixture(&fixture.control, ORIGIN);
    assert!(
        !fixture
            .control
            .status("quick")
            .unwrap()
            .registration_available
    );
    fixture
        .control
        .attempt
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .discovery_verified = Some(true);
    assert!(
        fixture
            .control
            .status("quick")
            .unwrap()
            .registration_available
    );
    for (header, value) in [
        ("host", "private.example"),
        ("origin", "https://other.example"),
    ] {
        assert_eq!(
            register()
                .add_header(header, value, true)
                .send(&service)
                .await
                .status_code,
            Some(StatusCode::FORBIDDEN)
        );
    }
    let ambiguous = register().add_header("origin", ORIGIN, true).add_header(
        "origin",
        "https://other.example",
        false,
    );
    assert_eq!(
        ambiguous.send(&service).await.status_code,
        Some(StatusCode::BAD_REQUEST)
    );
    let register_body =
        |body: Value| TestClient::post(&format!("{ORIGIN}/oauth/register")).json(&body);
    for bad in [
        json!({"redirect_uris":["http://remote.example/callback"]}),
        json!({"redirect_uris":[CALLBACK],"scope":"account:manage"}),
        json!({"redirect_uris":[CALLBACK],"scope":"admin"}),
    ] {
        assert_eq!(
            register_body(bad).send(&service).await.status_code,
            Some(StatusCode::BAD_REQUEST)
        );
    }
    assert_eq!(
        register_body(json!({"client_name":"x".repeat(17*1024)}))
            .send(&service)
            .await
            .status_code,
        Some(StatusCode::PAYLOAD_TOO_LARGE)
    );
    for _ in 0..16 {
        assert_eq!(
            register().send(&service).await.status_code,
            Some(StatusCode::CREATED)
        );
    }
    let limited = json_response(
        register().add_header("x-forwarded-for", "192.0.2.1", true),
        &service,
        StatusCode::TOO_MANY_REQUESTS,
    )
    .await;
    assert_eq!(limited["error"], "temporarily_unavailable");
    db.set_user_disabled(&entry.owner_user_id, true, chrono::Utc::now().timestamp())
        .unwrap();
    assert!(
        !fixture
            .control
            .status("quick")
            .unwrap()
            .registration_available
    );
    let metadata = json_response(discovery(), &service, StatusCode::OK).await;
    assert!(metadata.get("registration_endpoint").is_none());
    assert_eq!(
        register().send(&service).await.status_code,
        Some(StatusCode::SERVICE_UNAVAILABLE)
    );
}

#[tokio::test]
async fn gemini_cloudflare_dcr_browser_consent_pkce_mcp_call_refresh_and_revocation() {
    let _env = crate::auth::AuthEnvGuard::auth_required();
    let fixture = Fixture::new();
    enable_dcr(&fixture);
    let (_, entry) = activate_quick_fixture(&fixture.control, ORIGIN);
    let db = &fixture.control.db;
    let service = public_service(&fixture.control);
    let owner = db.get_user_by_id(&entry.owner_user_id).unwrap().unwrap();
    let pat = crate::test_support::seed_user_token_with_scopes(db, &owner, "admin");
    let resource = json_response(
        TestClient::get(&format!("{ORIGIN}/.well-known/oauth-protected-resource")),
        &service,
        StatusCode::OK,
    )
    .await;
    assert_eq!(resource["authorization_servers"], json!([ORIGIN]));
    let metadata = json_response(
        TestClient::get(&format!("{ORIGIN}/.well-known/oauth-authorization-server")),
        &service,
        StatusCode::OK,
    )
    .await;
    assert!(discovery_matches(ORIGIN, &resource, &metadata));
    assert!(!discovery_matches(
        "https://other.example",
        &resource,
        &metadata
    ));
    assert!(!discovery_matches(
        ORIGIN,
        &resource,
        &json!({"issuer": ORIGIN})
    ));
    assert_eq!(metadata["issuer"], ORIGIN);
    let endpoint = |name: &str| metadata[name].as_str().unwrap();
    let reg = json_response(TestClient::post(endpoint("registration_endpoint")).json(&json!({
        "redirect_uris":[CALLBACK], "client_name":"Gemini test", "grant_types":["authorization_code","refresh_token"], "scope":"runtime:read"
    })), &service, StatusCode::CREATED).await;
    let id = reg["client_id"].as_str().unwrap();
    let secret = reg["client_secret"].as_str().unwrap();
    assert!(reg.get("access_token").is_none());
    let client = db.get_oauth_client_by_client_id(id).unwrap().unwrap();
    assert_eq!(client.owner_user_id.as_deref(), Some(owner.id.as_str()));
    assert_eq!(client.dynamic_registration.unwrap().issuer, ORIGIN);
    assert!(db
        .oauth_client_matches_public_ingress(id, &entry.fence())
        .unwrap());

    let verifier = "p".repeat(43);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let params = form(&[
        ("client_id", id),
        ("response_type", "code"),
        ("redirect_uri", CALLBACK),
        ("scope", "runtime:read account:manage offline_access"),
        ("resource", resource["resource"].as_str().unwrap()),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
        ("state", "fixture"),
    ]);
    let authorize = format!("{}?{params}", endpoint("authorization_endpoint"));
    let login_page = TestClient::get(&authorize).send(&service).await;
    assert_eq!(login_page.status_code, Some(StatusCode::OK));
    let login = post_form(
        &format!("{ORIGIN}/oauth/authorize/login"),
        form(&[
            ("token", &pat),
            ("return_to", &format!("/oauth/authorize?{params}")),
        ]),
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
    let consent = TestClient::get(&authorize)
        .add_header("cookie", cookie, true)
        .send(&service)
        .await
        .take_string()
        .await
        .unwrap();
    assert!(consent.contains("Self-registered client") && consent.contains(CALLBACK));
    let approved = post_form(
        &format!("{ORIGIN}/oauth/authorize/consent"),
        format!("{params}&decision=allow"),
    )
    .add_header("cookie", cookie, true)
    .add_header("origin", ORIGIN, true)
    .send(&service)
    .await;
    assert_eq!(approved.status_code, Some(StatusCode::FOUND));
    let redirect = url::Url::parse(approved.headers()["location"].to_str().unwrap()).unwrap();
    let query: std::collections::HashMap<_, _> = redirect.query_pairs().collect();
    assert_eq!(query["iss"], ORIGIN);
    assert_eq!(query["state"], "fixture");
    let exchange = form(&[
        ("grant_type", "authorization_code"),
        ("code", &query["code"]),
        ("redirect_uri", CALLBACK),
        ("code_verifier", &verifier),
    ]);
    let tokens = json_response(
        post_form(endpoint("token_endpoint"), exchange).basic_auth(id, Some(secret)),
        &service,
        StatusCode::OK,
    )
    .await;
    assert_eq!(tokens["scope"], "runtime:read offline_access");
    let access = tokens["access_token"].as_str().unwrap();
    let refresh = tokens["refresh_token"].as_str().unwrap();
    for credential in [
        id,
        secret,
        query["code"].as_ref(),
        &pat,
        "private-bootstrap",
    ] {
        assert_eq!(
            mcp(credential, "tools/list", json!({}))
                .send(&service)
                .await
                .status_code,
            Some(StatusCode::UNAUTHORIZED)
        );
    }
    let listing = json_response(
        mcp(access, "tools/list", json!({})),
        &service,
        StatusCode::OK,
    )
    .await;
    assert!(!listing["result"]["tools"].as_array().unwrap().is_empty());
    assert!(
        fixture
            .control
            .status("quick")
            .unwrap()
            .observed_authorization
    );
    assert!(!fixture.control.status("quick").unwrap().observed_tool_call);
    let result = json_response(
        mcp(
            access,
            "tools/call",
            json!({"name":"get_runtime_status","arguments":{"summary_only":true}}),
        ),
        &service,
        StatusCode::OK,
    )
    .await;
    assert_eq!(result["result"]["structuredContent"]["success"], true);
    assert!(fixture.control.status("quick").unwrap().observed_tool_call);
    assert_eq!(
        serde_json::from_str::<Value>(result["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap(),
        result["result"]["structuredContent"]
    );

    let rotate = |token| {
        post_form(
            endpoint("token_endpoint"),
            form(&[("grant_type", "refresh_token"), ("refresh_token", token)]),
        )
        .basic_auth(id, Some(secret))
    };
    let rotated = json_response(rotate(refresh), &service, StatusCode::OK).await;
    let new_refresh = rotated["refresh_token"].as_str().unwrap();
    assert!(new_refresh != refresh);
    assert_eq!(
        rotate(refresh).send(&service).await.status_code,
        Some(StatusCode::BAD_REQUEST)
    );
    for token in [access, new_refresh] {
        json_response(
            post_form(endpoint("revocation_endpoint"), form(&[("token", token)]))
                .basic_auth(id, Some(secret)),
            &service,
            StatusCode::OK,
        )
        .await;
    }
    assert_eq!(
        rotate(new_refresh).send(&service).await.status_code,
        Some(StatusCode::BAD_REQUEST)
    );
    assert_eq!(
        mcp(access, "tools/list", json!({}))
            .send(&service)
            .await
            .status_code,
        Some(StatusCode::UNAUTHORIZED)
    );
    let management = Service::new(
        Router::new()
            .hoop(affix_state::inject(fixture.control.config.clone()))
            .hoop(affix_state::inject(db.clone()))
            .hoop(crate::AuthMiddleware)
            .push(
                Router::with_path("api/oauth/clients/update_scopes")
                    .post(crate::oauth_http::oauth_clients_update_scopes),
            )
            .push(
                Router::with_path("api/oauth/clients/revoke")
                    .post(crate::oauth_http::oauth_clients_revoke),
            ),
    );
    let revoke = || {
        TestClient::post("http://localhost/api/oauth/clients/revoke").json(&json!({"client_id":id}))
    };
    assert_eq!(
        revoke()
            .bearer_auth(rotated["access_token"].as_str().unwrap())
            .send(&management)
            .await
            .status_code,
        Some(StatusCode::UNAUTHORIZED)
    );
    let widen = TestClient::post("http://localhost/api/oauth/clients/update_scopes")
        .bearer_auth(&pat)
        .json(&json!({"client_id":id,"allowed_scopes":["account:manage"]}))
        .send(&management)
        .await;
    assert_eq!(widen.status_code, Some(StatusCode::BAD_REQUEST));
    assert_eq!(
        revoke()
            .bearer_auth(&pat)
            .send(&management)
            .await
            .status_code,
        Some(StatusCode::OK)
    );
    assert_eq!(
        mcp(
            rotated["access_token"].as_str().unwrap(),
            "tools/list",
            json!({})
        )
        .send(&service)
        .await
        .status_code,
        Some(StatusCode::UNAUTHORIZED)
    );
}

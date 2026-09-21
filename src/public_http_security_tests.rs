use salvo::http::StatusCode;
use salvo::prelude::*;
use salvo::test::TestClient;

use crate::public_http_security::{
    InvalidAuthLimiter, InvalidAuthRateLimitPolicy, PublicHttpSecurity,
};

#[handler]
async fn ok_handler(res: &mut Response) {
    res.status_code(StatusCode::OK);
    res.render(Text::Plain("ok"));
}

fn security_router() -> Router {
    Router::new()
        .hoop(PublicHttpSecurity)
        .push(Router::with_path("openapi.json").get(ok_handler))
        .push(Router::with_path("artifact-download").get(ok_handler))
        .push(Router::with_path("api/actions/runtime_status").post(ok_handler))
        .push(Router::with_path("api/tools/call").post(ok_handler))
        .push(Router::with_path("mcp").post(ok_handler))
        .push(Router::with_path("admin").get(ok_handler))
}

#[tokio::test]
async fn public_surface_allows_only_openapi_and_actions_while_loopback_stays_full() {
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBPI_PUBLIC_URL", "https://webpi.example");
    env.set("WEBPI_PUBLIC_ACTIONS_ONLY", "true");
    let service = Service::new(security_router());

    let openapi = TestClient::get("http://webpi.example/openapi.json")
        .add_header("host", "webpi.example", true)
        .send(&service)
        .await;
    assert_eq!(openapi.status_code, Some(StatusCode::OK));

    let artifact_download = TestClient::get("http://webpi.example/artifact-download?id=opaque")
        .add_header("host", "webpi.example", true)
        .send(&service)
        .await;
    assert_eq!(artifact_download.status_code, Some(StatusCode::OK));

    let action = TestClient::post("http://webpi.example/api/actions/runtime_status")
        .add_header("host", "webpi.example", true)
        .send(&service)
        .await;
    assert_eq!(action.status_code, Some(StatusCode::OK));

    for url in [
        "http://webpi.example/api/tools/call",
        "http://webpi.example/mcp",
        "http://webpi.example/admin",
    ] {
        let response = if url.ends_with("/admin") {
            TestClient::get(url)
                .add_header("host", "webpi.example", true)
                .send(&service)
                .await
        } else {
            TestClient::post(url)
                .add_header("host", "webpi.example", true)
                .send(&service)
                .await
        };
        assert_eq!(response.status_code, Some(StatusCode::NOT_FOUND), "{url}");
    }

    let loopback = TestClient::post("http://127.0.0.1/api/tools/call")
        .send(&service)
        .await;
    assert_eq!(loopback.status_code, Some(StatusCode::OK));
}

#[tokio::test]
async fn cloudflare_connecting_ip_marks_tunnel_request_public_even_if_origin_host_is_loopback() {
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBPI_PUBLIC_URL", "https://webpi.example");
    env.set("WEBPI_PUBLIC_ACTIONS_ONLY", "true");
    let service = Service::new(security_router());

    let response = TestClient::post("http://127.0.0.1/api/tools/call")
        .add_header("cf-connecting-ip", "203.0.113.9", true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::NOT_FOUND));
}

#[test]
fn invalid_auth_limiter_is_bounded_and_only_penalizes_after_the_configured_budget() {
    let policy = InvalidAuthRateLimitPolicy {
        max_failures: 3,
        window_secs: 60,
        penalty_secs: 90,
    };
    let mut limiter = InvalidAuthLimiter::default();

    assert_eq!(limiter.record_failure_at("203.0.113.9", 100, policy), None);
    assert_eq!(limiter.record_failure_at("203.0.113.9", 101, policy), None);
    assert_eq!(limiter.record_failure_at("203.0.113.9", 102, policy), None);
    assert_eq!(
        limiter.record_failure_at("203.0.113.9", 103, policy),
        Some(90)
    );
    assert_eq!(
        limiter.record_failure_at("203.0.113.9", 120, policy),
        Some(73)
    );

    // A different client is independent and still receives its full budget.
    assert_eq!(limiter.record_failure_at("198.51.100.7", 120, policy), None);

    // Once the penalty and original window have elapsed the client gets a fresh budget.
    assert_eq!(limiter.record_failure_at("203.0.113.9", 194, policy), None);
}

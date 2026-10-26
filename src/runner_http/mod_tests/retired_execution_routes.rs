use super::*;
use salvo::test::{ResponseExt, TestClient};
use salvo::Service;

#[tokio::test]
async fn retired_shell_execution_routes_are_unmounted() {
    let registry = Arc::new(RunnerRegistry::default());
    let service = Service::new(
        Router::with_path("api")
            .hoop(affix_state::inject(registry.clone()))
            .hoop(affix_state::inject(auth_context(None, true)))
            .push(routes()),
    );
    for path in [
        "/api/shell/run",
        "/api/shell/file",
        "/api/shell/job",
        "/api/shell/jobs/status",
        "/api/shell/jobs/log",
        "/api/shell/jobs/stop",
        "/api/shell/jobs/list",
    ] {
        let response = TestClient::post(format!("http://localhost{path}"))
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(
            response.status_code.unwrap_or(StatusCode::OK),
            StatusCode::NOT_FOUND,
            "retired route {path} must not dispatch",
        );
    }
    assert!(registry.list_runners().await.is_empty());

    // Exercise the exact same production subtree: every retained transport
    // endpoint still reaches its handler and rejects malformed input.
    for leaf in [
        "register",
        "offline",
        "poll",
        "result",
        "persistent_shell_result",
        "job_update",
    ] {
        let mut response = TestClient::post(format!("http://localhost/api/shell/agent/{leaf}"))
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(
            response.status_code.unwrap_or(StatusCode::OK),
            StatusCode::BAD_REQUEST,
            "Runner transport {leaf} must remain mounted",
        );
        let body: serde_json::Value = response.take_json().await.unwrap();
        assert_eq!(body["success"], false, "{leaf}: {body}");
    }
}

#[tokio::test]
async fn polling_registration_keeps_transport_identity_authority() {
    let registry = Arc::new(RunnerRegistry::default());
    let service = Service::new(
        Router::with_path("api")
            .hoop(affix_state::inject(registry.clone()))
            .hoop(affix_state::inject(auth_context(Some("owner"), false)))
            .push(routes()),
    );
    let registration = runner_registration("retired-route-check", "instance", Vec::new());
    let mut response = TestClient::post("http://localhost/api/shell/agent/register")
        .json(&registration)
        .send(&service)
        .await;
    assert_eq!(
        response.status_code.unwrap_or(StatusCode::OK),
        StatusCode::FORBIDDEN,
    );
    let body: serde_json::Value = response.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(registry.list_runners().await.is_empty());
}

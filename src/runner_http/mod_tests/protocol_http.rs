use super::*;

#[tokio::test]
async fn polling_http_register_requires_generation() {
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;

    let registry = Arc::new(RunnerRegistry::default());
    let service = Service::new(
        Router::new()
            .hoop(affix_state::inject(registry.clone()))
            .hoop(affix_state::inject(auth_context(None, true)))
            .push(Router::with_path("api/shell/agent/register").post(runner_register)),
    );
    let mut response = TestClient::post("http://localhost/api/shell/agent/register")
        .json(&json!({
            "client_id": "polling-missing-generation",
            "agent_instance_id": "inst",
            "capabilities": {"shell": true}
        }))
        .send(&service)
        .await;
    assert_eq!(
        response.status_code.unwrap_or(StatusCode::OK),
        StatusCode::BAD_REQUEST
    );
    let body: serde_json::Value = response.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|error| error.contains("agent_protocol_generation")),
        "{body:?}"
    );
    assert!(registry.list_runners().await.is_empty());
}

#[tokio::test]
async fn polling_http_register_accepts_generation_two() {
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;

    let registry = Arc::new(RunnerRegistry::default());
    let service = Service::new(
        Router::new()
            .hoop(affix_state::inject(registry.clone()))
            .hoop(affix_state::inject(auth_context(None, true)))
            .push(Router::with_path("api/shell/agent/register").post(runner_register)),
    );
    let mut response = TestClient::post("http://localhost/api/shell/agent/register")
        .json(&json!({
            "client_id": "polling-generation-two",
            "agent_instance_id": "inst",
            "agent_protocol_generation": RUNNER_PROTOCOL_GENERATION_V2.get(),
            "capabilities": crate::test_support::current_runner_capabilities(RunnerCapabilities::default())
        }))
        .send(&service)
        .await;
    assert_eq!(
        response.status_code.unwrap_or(StatusCode::OK),
        StatusCode::OK
    );
    let body: serde_json::Value = response.take_json().await.unwrap();
    assert_eq!(body["success"], true);
    let view = registry
        .get_runner_view("polling-generation-two")
        .await
        .unwrap();
    assert_eq!(
        view.runner_protocol_generation,
        RUNNER_PROTOCOL_GENERATION_V2
    );
    assert_eq!(view.transport, TRANSPORT_POLLING);
    assert!(view.projects.is_empty());
    assert_eq!(
        view.project_inventory
            .as_ref()
            .map(|status| status.sync_state.as_str()),
        Some("pending")
    );
}

#[tokio::test]
async fn polling_http_offline_releases_only_the_matching_active_instance() {
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;

    let registry = Arc::new(RunnerRegistry::default());
    registry
        .register(runner_registration("polling-offline", "inst-a", Vec::new()))
        .await
        .unwrap();
    let service = Service::new(
        Router::new()
            .hoop(affix_state::inject(registry.clone()))
            .hoop(affix_state::inject(auth_context(None, true)))
            .push(Router::with_path("api/shell/agent/offline").post(runner_offline)),
    );

    let mut response = TestClient::post("http://localhost/api/shell/agent/offline")
        .json(&json!({
            "client_id": "polling-offline",
            "agent_instance_id": "inst-a"
        }))
        .send(&service)
        .await;
    assert_eq!(
        response.status_code.unwrap_or(StatusCode::OK),
        StatusCode::OK
    );
    assert_eq!(
        response.take_json::<serde_json::Value>().await.unwrap()["success"],
        true
    );
    assert!(
        !registry
            .get_runner_view("polling-offline")
            .await
            .unwrap()
            .connected
    );

    let replacement = registry
        .register(runner_registration("polling-offline", "inst-b", Vec::new()))
        .await
        .unwrap();
    assert!(replacement.connected);
    let mut stale = TestClient::post("http://localhost/api/shell/agent/offline")
        .json(&json!({
            "client_id": "polling-offline",
            "agent_instance_id": "inst-a"
        }))
        .send(&service)
        .await;
    assert_eq!(stale.status_code.unwrap_or(StatusCode::OK), StatusCode::OK);
    assert_eq!(
        stale.take_json::<serde_json::Value>().await.unwrap()["success"],
        true
    );
    let current = registry.get_runner_view("polling-offline").await.unwrap();
    assert_eq!(current.runner_instance_id, "inst-b");
    assert!(current.connected);
}

use super::*;

fn response_routes() -> Router {
    Router::new()
        .push(Router::with_path("overview").post(overview))
        .push(Router::with_path("agents").post(communication_agents))
        .push(Router::with_path("agent/create").post(communication_agent_create))
        .push(Router::with_path("goal").post(goal_handler))
        .push(Router::with_path("trace").post(trace::read))
        .push(Router::with_path("extensions").post(workspace::extensions))
}

#[tokio::test]
async fn console_context_and_scope_checks_precede_body_validation() {
    let missing_runtime = Service::new(response_routes());
    let missing_auth = Service::new(
        Router::new()
            .hoop(affix_state::inject(test_runtime()))
            .push(response_routes()),
    );
    for path in [
        "overview",
        "agents",
        "agent/create",
        "goal",
        "trace",
        "extensions",
    ] {
        for service in [&missing_runtime, &missing_auth] {
            let mut response = TestClient::post(format!("http://localhost/{path}"))
                .json(&json!(false))
                .send(service)
                .await;
            assert_eq!(
                response.status_code,
                Some(StatusCode::INTERNAL_SERVER_ERROR),
                "{path}"
            );
            assert_eq!(
                response.take_json::<Value>().await.unwrap(),
                json!({
                    "status": 500, "error": "Runtime Console unavailable",
                }),
                "{path}"
            );
        }
    }

    let unauthorized = Service::new(
        Router::new()
            .hoop(affix_state::inject(test_runtime()))
            .hoop(affix_state::inject(scoped_oauth(&[])))
            .push(response_routes()),
    );
    for (path, message) in [
        ("agents", "Communication read access required"),
        (
            "agent/create",
            "Communication read and manage access required",
        ),
        ("goal", "Communication read access required"),
        ("trace", "Administrator diagnostic access required"),
    ] {
        let mut response = TestClient::post(format!("http://localhost/{path}"))
            .json(&json!(false))
            .send(&unauthorized)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::FORBIDDEN), "{path}");
        assert_eq!(
            response.take_json::<Value>().await.unwrap(),
            json!({
                "status": 403, "error": message,
            }),
            "{path}"
        );
    }
}

#[tokio::test]
async fn console_parse_failures_keep_the_console_error_envelope() {
    let service = Service::new(
        Router::new()
            .hoop(affix_state::inject(test_runtime()))
            .hoop(affix_state::inject(test_bootstrap_auth()))
            .push(response_routes()),
    );
    for path in [
        "overview",
        "agents",
        "agent/create",
        "goal",
        "trace",
        "extensions",
    ] {
        let mut response = TestClient::post(format!("http://localhost/{path}"))
            .json(&json!(false))
            .send(&service)
            .await;
        assert_eq!(
            response.status_code,
            Some(StatusCode::BAD_REQUEST),
            "{path}"
        );
        assert!(response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("application/json"));
        assert_eq!(
            response.take_json::<Value>().await.unwrap(),
            json!({
                "status": 400, "error": "Invalid request",
            }),
            "{path}"
        );
    }
}

#[tokio::test]
async fn communication_failures_keep_tool_output_in_the_http_response() {
    let service = Service::new(
        Router::new()
            .hoop(affix_state::inject(test_runtime()))
            .hoop(affix_state::inject(scoped_oauth(&[
                SCOPE_COMMUNICATION_READ,
            ])))
            .push(response_routes()),
    );
    let mut response = TestClient::post("http://localhost/agents")
        .json(&json!({}))
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::SERVICE_UNAVAILABLE));
    let body: Value = response.take_json().await.unwrap();
    assert_eq!(body["error_kind"], "communication_store_unavailable");
    assert!(body.get("status").is_none());
    assert!(body.get("output").is_none());
}

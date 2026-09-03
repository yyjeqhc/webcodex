use super::*;

#[tokio::test]
async fn raw_shell_run_wait_timeout_preserves_known_dispatch_evidence() {
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;

    let client_id = "raw-shell-timeout";
    let registry = Arc::new(RunnerRegistry::default());
    let mut registration = runner_registration(
        client_id,
        "inst",
        vec![project_summary("webcodex", "/tmp/webcodex")],
    );
    registration.capabilities =
        crate::test_support::current_runner_capabilities(RunnerCapabilities {
            shell: true,
            ..Default::default()
        });
    registry.register(registration).await.unwrap();

    let service = Service::new(
        Router::new()
            .hoop(affix_state::inject(registry.clone()))
            .hoop(affix_state::inject(auth_context(None, true)))
            .push(Router::with_path("api/shell/run").post(shell_run)),
    );
    let response = TestClient::post("http://localhost/api/shell/run")
        .json(&json!({
            "client_id": client_id,
            "cwd": null,
            "command": "echo hi",
            "stdin": null,
            "timeout_secs": 5,
            "wait_timeout_secs": 1
        }))
        .send(&service);
    let poll = async {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if let Some(request) = registry
                .poll(RunnerPollRequest {
                    client_id: client_id.to_string(),
                    runner_instance_id: "inst".to_string(),
                })
                .await
                .unwrap()
            {
                return request;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "raw shell request was not dispatched within 2 seconds"
            );
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    };
    let (mut response, request) = tokio::join!(response, poll);
    assert_eq!(request.kind, "run_shell");
    assert_eq!(response.status_code, Some(StatusCode::REQUEST_TIMEOUT));
    let body = response
        .take_json::<serde_json::Value>()
        .await
        .expect("raw shell timeout JSON");
    assert_eq!(body["request_dispatched"], true);
    assert!(
        body.get("command_execution_state").is_none(),
        "the server must not fabricate Runner lifecycle evidence: {body}"
    );
}

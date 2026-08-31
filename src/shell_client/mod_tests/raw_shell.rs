use super::*;

#[tokio::test]
async fn raw_shell_run_wait_timeout_preserves_known_dispatch_evidence() {
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;

    let client_id = "raw-shell-timeout";
    let registry = Arc::new(ShellClientRegistry::default());
    let mut registration = runner_registration(
        client_id,
        "inst",
        vec![project_summary("webcodex", "/tmp/webcodex")],
    );
    registration.capabilities = Some(crate::test_support::current_runner_capabilities(
        ShellClientCapabilities {
            shell: true,
            ..Default::default()
        },
    ));
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
                .poll(ShellAgentPollRequest {
                    client_id: client_id.to_string(),
                    agent_instance_id: "inst".to_string(),
                    projects: None,
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

#[test]
fn validate_run_request_uses_the_internal_raw_shell_wire_bound() {
    let exact = ShellRunRequest {
        client_id: "client-1".to_string(),
        cwd: None,
        command: "x".repeat(crate::shell_protocol::RAW_SHELL_WIRE_MAX_BYTES),
        stdin: None,
        timeout_secs: 10,
        wait_timeout_secs: 1,
    };
    validate_run_request(&exact).expect("wire-bound command accepted");

    let mut oversized = exact;
    oversized.command.push('x');
    let error = validate_run_request(&oversized).unwrap_err();
    assert!(error.contains("Runner wire envelope"), "{error}");
}

#[test]
fn validate_run_request_allows_bounded_stdin_beyond_command_limit() {
    let body = ShellRunRequest {
        client_id: "client-1".to_string(),
        cwd: None,
        command: "cat >/dev/null".to_string(),
        stdin: Some("x".repeat(crate::shell_protocol::RAW_SHELL_COMMAND_MAX_BYTES + 1024)),
        timeout_secs: 10,
        wait_timeout_secs: 1,
    };
    validate_run_request(&body).expect("stdin has its own larger bound");
}

#[test]
fn validate_run_request_rejects_oversized_stdin() {
    let body = ShellRunRequest {
        client_id: "client-1".to_string(),
        cwd: None,
        command: "cat >/dev/null".to_string(),
        stdin: Some("x".repeat(MAX_RUN_STDIN_BYTES + 1)),
        timeout_secs: 10,
        wait_timeout_secs: 1,
    };
    let err = validate_run_request(&body).unwrap_err();
    assert!(err.contains("stdin is too large"), "got: {}", err);
}

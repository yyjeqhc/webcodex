use super::*;

#[tokio::test]
async fn communication_canonical_not_found_errors_render_as_http_404() {
    for error_kind in [
        "agent_not_found",
        "endpoint_not_found",
        "conversation_not_found",
        "message_not_found",
        "reply_message_not_found",
        "delivery_not_found",
    ] {
        let output = json!({
            "error_kind": error_kind,
            "message": "Communication resource does not exist",
            "current_profile_revision": null,
            "state_changed": false,
        });
        let result =
            ToolResult::err_with_output("Communication resource does not exist", output.clone())
                .with_recovery(RecoveryKind::FixInput);
        let expected_output = result.output.clone();
        let mut response = Response::new();
        render_communication_result(&mut response, result);
        assert_eq!(response.status_code, Some(StatusCode::NOT_FOUND));
        let body = response.take_json::<Value>().await.unwrap();
        assert_eq!(body, expected_output, "{error_kind}");
    }
}

#[test]
fn communication_scope_checks_are_independent_from_project_and_session_authority() {
    let project_and_session = scoped_oauth(&[
        SCOPE_PROJECT_READ,
        SCOPE_RUNTIME_READ,
        SCOPE_SESSION_COLLABORATE,
    ]);
    assert_eq!(
        require_communication_read(&project_and_session),
        Err(RuntimeConsoleError::Request {
            status: 403,
            message: "Communication read access required",
        })
    );
    assert_eq!(
        require_communication_manage(&project_and_session),
        Err(RuntimeConsoleError::Request {
            status: 403,
            message: "Communication read and manage access required",
        })
    );

    let read_only = scoped_oauth(&[SCOPE_COMMUNICATION_READ]);
    assert_eq!(require_communication_read(&read_only), Ok(()));
    assert_eq!(
        require_communication_manage(&read_only),
        Err(RuntimeConsoleError::Request {
            status: 403,
            message: "Communication read and manage access required",
        })
    );

    let communication = scoped_oauth(&[SCOPE_COMMUNICATION_READ, SCOPE_COMMUNICATION_MANAGE]);
    assert_eq!(require_communication_read(&communication), Ok(()));
    assert_eq!(require_communication_manage(&communication), Ok(()));
}

#[tokio::test]
async fn durable_agent_chat_http_vertical_slice_preserves_provenance_and_inbox_state() {
    let shared_key = "communication-http-secret";
    let (_tmp, service) = hosted_communication_service(shared_key);

    let (status, first_agent) = post_communication(
        &service,
        shared_key,
        "agent/create",
        json!({
            "handle": "reviewer",
            "display_name": "Reviewer",
            "description": "Reviews durable architecture",
            "specialty_labels": ["rust", "architecture"],
            "idempotency_key": "http-agent-a"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let agent_a = first_agent["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string();

    let (status, second_agent) = post_communication(
        &service,
        shared_key,
        "agent/create",
        json!({
            "handle": "reviewer",
            "display_name": "Reviewer",
            "description": "Same mutable card, different canonical identity",
            "specialty_labels": ["review"],
            "idempotency_key": "http-agent-b"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let agent_b = second_agent["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(agent_a, agent_b);

    let (status, endpoint_a_body) = post_communication(
        &service,
        shared_key,
        "endpoint/attach",
        json!({
            "agent_id": agent_a,
            "host": "Runtime Console Test",
            "client_attachment_id": "window-a",
            "idempotency_key": "http-endpoint-a"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let endpoint_a = endpoint_a_body["endpoint"]["endpoint_id"]
        .as_str()
        .unwrap()
        .to_string();
    let generation_a = endpoint_a_body["endpoint"]["controller_generation"]
        .as_i64()
        .unwrap();

    let (status, endpoint_b_body) = post_communication(
        &service,
        shared_key,
        "endpoint/attach",
        json!({
            "agent_id": agent_b,
            "host": "Runtime Console Test",
            "client_attachment_id": "window-b",
            "idempotency_key": "http-endpoint-b"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let endpoint_b = endpoint_b_body["endpoint"]["endpoint_id"]
        .as_str()
        .unwrap()
        .to_string();
    let generation_b = endpoint_b_body["endpoint"]["controller_generation"]
        .as_i64()
        .unwrap();

    let (status, conversation_body) = post_communication(
        &service,
        shared_key,
        "conversation/create",
        json!({
            "title": "HTTP architecture room",
            "agent_ids": [agent_a, agent_b],
            "idempotency_key": "http-conversation"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let conversation_id = conversation_body["conversation"]["conversation"]["conversation_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        conversation_body["conversation"]["participants"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    let human_payload = json!({
        "conversation_id": conversation_id,
        "body": "Human to both Agents",
        "idempotency_key": "http-human-message"
    });
    let (status, human_message) =
        post_communication(&service, shared_key, "message/post", human_payload.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(human_message["message"]["seq"], 1);
    assert_eq!(
        human_message["message"]["author"]["participant_kind"],
        "human"
    );
    assert_eq!(
        human_message["message"]["deliveries"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let human_message_id = human_message["message"]["message_id"]
        .as_str()
        .unwrap()
        .to_string();

    let (status, replay) =
        post_communication(&service, shared_key, "message/post", human_payload).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["state_changed"], false);
    assert_eq!(replay["message"]["message_id"], human_message_id);

    let (status, agent_message) = post_communication(
        &service,
        shared_key,
        "message/post",
        json!({
            "conversation_id": conversation_id,
            "body": "Agent A to Agent B",
            "author_agent_id": agent_a,
            "endpoint_id": endpoint_a,
            "expected_controller_generation": generation_a,
            "recipient_agent_ids": [agent_b],
            "reply_to": human_message_id,
            "idempotency_key": "http-agent-message"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(agent_message["message"]["seq"], 2);
    assert_eq!(
        agent_message["message"]["author"]["participant_kind"],
        "agent"
    );
    assert_eq!(agent_message["message"]["author"]["agent_id"], agent_a);
    assert_eq!(
        agent_message["message"]["deliveries"][0]["recipient_agent_id"],
        agent_b
    );

    let (status, transcript) = post_communication(
        &service,
        shared_key,
        "conversation",
        json!({
            "conversation_id": conversation_id,
            "after_seq": 0,
            "limit": 10
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(transcript["conversation"]["message_count"], 2);
    let sequences = transcript["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|message| message["seq"].as_i64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(sequences, vec![1, 2]);

    let (status, inbox) = post_communication(
        &service,
        shared_key,
        "inbox",
        json!({
            "agent_id": agent_b,
            "endpoint_id": endpoint_b,
            "expected_controller_generation": generation_b,
            "after_delivery_order": 0,
            "limit": 10
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(inbox["total_queued_count"], 2);
    let delivery_ids = inbox["deliveries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|delivery| delivery["delivery_id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();

    let consume_payload = json!({
        "agent_id": agent_b,
        "endpoint_id": endpoint_b,
        "expected_controller_generation": generation_b,
        "delivery_ids": delivery_ids
    });
    let (status, consumed) = post_communication(
        &service,
        shared_key,
        "inbox/consume",
        consume_payload.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(consumed["state_changed"], true);
    assert_eq!(
        consumed["consumed_delivery_ids"].as_array().unwrap().len(),
        2
    );
    let (status, consumed_retry) =
        post_communication(&service, shared_key, "inbox/consume", consume_payload).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(consumed_retry["state_changed"], false);
    assert_eq!(
        consumed_retry["already_consumed_delivery_ids"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let (status, agents) = post_communication(
        &service,
        shared_key,
        "agents",
        json!({"offset": 0, "limit": 10}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let agent_rows = agents["agents"].as_array().unwrap();
    assert_eq!(
        agent_rows
            .iter()
            .find(|agent| agent["agent_id"] == agent_a)
            .unwrap()["queued_delivery_count"],
        1
    );
    assert_eq!(
        agent_rows
            .iter()
            .find(|agent| agent["agent_id"] == agent_b)
            .unwrap()["queued_delivery_count"],
        0
    );

    let cross_origin =
        TestClient::post("http://localhost/api/runtime-console/communication/agents")
            .bearer_auth(shared_key)
            .add_header("host", "localhost", true)
            .add_header("origin", "http://attacker.example", true)
            .json(&json!({}))
            .send(&service)
            .await;
    assert_eq!(cross_origin.status_code, Some(StatusCode::FORBIDDEN));
}

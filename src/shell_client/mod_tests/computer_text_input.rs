use super::*;

#[tokio::test]
async fn computer_text_input_enqueue_requires_exact_owner_and_independent_capability() {
    let registry = ShellClientRegistry::default();
    let alice = auth_context(Some("alice"), false);
    let bob = auth_context(Some("bob"), false);
    let text = "隐私输入🙂";
    let payload = serde_json::json!({
        "surface_id": "surface_test",
        "element_id": "element_test",
        "text": text,
    })
    .to_string();

    register_computer_test_client(
        &registry,
        "computer-input",
        "alice",
        true,
        true,
        true,
        false,
    )
    .await;
    let error = registry
        .enqueue_computer(
            "computer-input".to_string(),
            "computer_input_text",
            payload.clone(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not support computer_text_input"));

    register_computer_test_client(&registry, "computer-input", "alice", true, true, true, true)
        .await;
    let error = registry
        .enqueue_computer(
            "computer-input".to_string(),
            "computer_input_text",
            payload.clone(),
            "bob".to_string(),
            Some(&bob),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("unknown shell client"), "{error}");
    assert!(!error.contains("owned by"), "{error}");

    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-input".to_string(),
            "computer_input_text",
            payload.clone(),
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-input".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("queued computer text input request");
    assert_eq!(request.kind, "computer_input_text");
    assert_eq!(request.stdin.as_deref(), Some(payload.as_str()));
    assert!(request.command.is_empty());
    let preview = super::jobs::request_preview(&request);
    assert!(preview.is_empty());
    assert!(!preview.contains(text));
}

#[tokio::test]
async fn computer_text_input_enqueue_preserves_max_utf8_text_after_json_escaping() {
    let registry = ShellClientRegistry::default();
    let alice = auth_context(Some("alice"), false);
    register_computer_test_client(
        &registry,
        "computer-input-escaped",
        "alice",
        true,
        true,
        true,
        true,
    )
    .await;

    let text = "\u{1}".repeat(2048);
    let payload = serde_json::json!({
        "surface_id": "surface_test",
        "element_id": "element_test",
        "text": text,
    })
    .to_string();
    assert!(payload.len() > crate::shell_protocol::SHELL_COMPUTER_REQUEST_PAYLOAD_MAX_BYTES);
    assert!(payload.len() <= crate::shell_protocol::SHELL_COMPUTER_TEXT_INPUT_PAYLOAD_MAX_BYTES);

    let (_request_id, _rx) = registry
        .enqueue_computer(
            "computer-input-escaped".to_string(),
            "computer_input_text",
            payload,
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "computer-input-escaped".to_string(),
            agent_instance_id: "computer-inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .expect("queued escaped computer text input request");
    let decoded: serde_json::Value =
        serde_json::from_str(request.stdin.as_deref().expect("computer input payload")).unwrap();
    assert_eq!(decoded["text"].as_str(), Some(text.as_str()));

    let oversized =
        "x".repeat(crate::shell_protocol::SHELL_COMPUTER_TEXT_INPUT_PAYLOAD_MAX_BYTES + 1);
    let error = registry
        .enqueue_computer(
            "computer-input-escaped".to_string(),
            "computer_input_text",
            oversized,
            "alice".to_string(),
            Some(&alice),
            5,
        )
        .await
        .unwrap_err();
    assert!(error.contains("payload is invalid or too large"));
}

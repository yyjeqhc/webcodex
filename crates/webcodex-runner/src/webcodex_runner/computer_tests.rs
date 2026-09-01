use super::*;

fn request(kind: &str, payload: &str) -> ShellAgentShellRequest {
    ShellAgentShellRequest {
        request_id: "computer-test".to_string(),
        client_id: "runner".to_string(),
        kind: kind.to_string(),
        job_id: None,
        cwd: None,
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: String::new(),
        process: None,
        script: None,
        stdin: Some(payload.to_string()),
        timeout_secs: 5,
        requested_by: "test".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        mcp_gateway: None,
        coding_agent: None,
        persistent_shell: None,
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
#[test]
fn computer_unsupported_platform_fails_closed_without_shell_fallback() {
    let result = handle_computer_request(&request("computer_list_windows", r#"{"limit":1}"#));
    assert_eq!(result.exit_code, None);
    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| error.starts_with("unsupported_platform:")));
}

#[test]
fn computer_unknown_surface_is_stale_before_platform_capture() {
    let result = handle_computer_request(&request(
        "computer_snapshot",
        r#"{"surface_id":"surface_missing"}"#,
    ));
    assert_eq!(result.exit_code, None);
    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| error.starts_with("stale_surface:")));
}

#[test]
fn computer_request_kinds_remain_closed() {
    for kind in [
        "computer_list_windows",
        "computer_list_applications",
        "computer_launch_application",
        "computer_list_displays",
        "computer_snapshot_display",
        "computer_read_clipboard",
        "computer_write_clipboard",
        "computer_pointer_move",
        "computer_pointer_click",
        "computer_snapshot",
        "computer_snapshot_region",
        "computer_accessibility_status",
        "computer_accessibility_tree",
        "computer_element_state",
        "computer_activate_window",
        "computer_control",
        "computer_scroll_to_element",
        "computer_key_input",
        "computer_input_text",
    ] {
        assert!(is_computer_request_kind(kind), "{kind}");
    }
    for kind in ["computer_unknown", "computer_snapshot_extra", "shell"] {
        assert!(!is_computer_request_kind(kind), "{kind}");
    }
}

#[test]
fn computer_snapshot_region_payload_is_closed_and_typed() {
    let exact = serde_json::json!({
        "surface_id": "surface_test",
        "region": {"x": 1, "y": 2, "width": 3, "height": 4},
        "max_width": 100,
        "max_height": null
    });
    assert!(ensure_exact_payload_fields(
        &exact,
        &["surface_id", "region", "max_width", "max_height"]
    )
    .is_ok());
    let region = optional_snapshot_region(&exact).unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(region).unwrap(),
        serde_json::json!({"x": 1, "y": 2, "width": 3, "height": 4})
    );
    assert_eq!(
        optional_snapshot_dimension(&exact, "max_width").unwrap(),
        Some(100)
    );
    assert_eq!(
        optional_snapshot_dimension(&exact, "max_height").unwrap(),
        None
    );

    let extra = serde_json::json!({
        "surface_id": "surface_test",
        "region": {"x": 1, "y": 2, "width": 3, "height": 4},
        "max_width": 100,
        "max_height": null,
        "quality": 99
    });
    assert!(ensure_exact_payload_fields(
        &extra,
        &["surface_id", "region", "max_width", "max_height"]
    )
    .is_err());
    let nested_extra = serde_json::json!({
        "region": {"x": 1, "y": 2, "width": 3, "height": 4, "global": true}
    });
    assert!(optional_snapshot_region(&nested_extra).is_err());
}

#[test]
fn computer_element_state_payload_is_exact_surface_and_element_only() {
    let exact = serde_json::json!({
        "surface_id": "surface_test",
        "element_id": "element_test"
    });
    assert!(ensure_exact_payload_fields(&exact, &["surface_id", "element_id"]).is_ok());
    for extra in [
        serde_json::json!({"surface_id": "surface_test", "element_id": "element_test", "value": true}),
        serde_json::json!({"surface_id": "surface_test", "element_id": "element_test", "action": "focus"}),
        serde_json::json!({"surface_id": "surface_test", "element_id": "element_test", "refresh": true}),
    ] {
        assert!(ensure_exact_payload_fields(&extra, &["surface_id", "element_id"]).is_err());
    }
}

#[test]
fn computer_activate_window_payload_is_exact_surface_only() {
    let exact = serde_json::json!({"surface_id": "surface_test"});
    assert!(ensure_exact_payload_fields(&exact, &["surface_id"]).is_ok());
    for extra in [
        serde_json::json!({"surface_id": "surface_test", "application": "Finder"}),
        serde_json::json!({"surface_id": "surface_test", "pid": 42}),
        serde_json::json!({"surface_id": "surface_test", "path": "/Applications/Finder.app"}),
        serde_json::json!({"surface_id": "surface_test", "command": "open -a Finder"}),
    ] {
        assert!(ensure_exact_payload_fields(&extra, &["surface_id"]).is_err());
    }
}

#[test]
fn computer_control_payload_rejects_semantic_extra_fields() {
    let exact = serde_json::json!({
        "surface_id": "surface_test",
        "element_id": "element_test",
        "action": "press"
    });
    assert!(ensure_exact_payload_fields(&exact, &["surface_id", "element_id", "action"]).is_ok());
    let extra = serde_json::json!({
        "surface_id": "surface_test",
        "element_id": "element_test",
        "action": "press",
        "script": "ignored"
    });
    assert!(ensure_exact_payload_fields(&extra, &["surface_id", "element_id", "action"]).is_err());
}

#[test]
fn computer_text_input_payload_is_closed() {
    let exact = serde_json::json!({
        "surface_id": "surface_test",
        "element_id": "element_test",
        "text": "你好🙂"
    });
    assert!(ensure_exact_payload_fields(&exact, &["surface_id", "element_id", "text"]).is_ok());
    let extra = serde_json::json!({
        "surface_id": "surface_test",
        "element_id": "element_test",
        "text": "hello",
        "action": "focus"
    });
    assert!(ensure_exact_payload_fields(&extra, &["surface_id", "element_id", "text"]).is_err());
}

#[test]
fn application_request_payloads_remain_closed() {
    assert!(ensure_exact_payload_fields(&serde_json::json!({"limit": 2}), &["limit"]).is_ok());
    assert!(ensure_exact_payload_fields(
        &serde_json::json!({"application_id": "application_0123456789abcdef0123456789abcdef"}),
        &["application_id"],
    )
    .is_ok());
    for extra in ["path", "argv", "cwd", "environment", "command", "url"] {
        let mut payload = serde_json::json!({
            "application_id": "application_0123456789abcdef0123456789abcdef"
        });
        payload
            .as_object_mut()
            .unwrap()
            .insert(extra.to_string(), serde_json::json!("x"));
        assert!(ensure_exact_payload_fields(&payload, &["application_id"]).is_err());
    }
}

#[test]
fn display_request_payloads_remain_closed() {
    assert!(ensure_exact_payload_fields(&serde_json::json!({"limit": 2}), &["limit"]).is_ok());
    let exact = serde_json::json!({
        "display_id": "display_0123456789abcdef0123456789abcdef",
        "max_width": 800,
        "max_height": null,
    });
    assert!(
        ensure_exact_payload_fields(&exact, &["display_id", "max_width", "max_height"]).is_ok()
    );
    for extra in [
        "region",
        "x",
        "y",
        "global_x",
        "pointer",
        "click",
        "monitor_id",
    ] {
        let mut payload = exact.clone();
        payload
            .as_object_mut()
            .unwrap()
            .insert(extra.to_string(), serde_json::json!(1));
        assert!(
            ensure_exact_payload_fields(&payload, &["display_id", "max_width", "max_height"])
                .is_err()
        );
    }
}

#[test]
fn pointer_request_payloads_remain_snapshot_fenced_and_closed() {
    let exact = serde_json::json!({
        "display_id": "display_0123456789abcdef0123456789abcdef",
        "snapshot_generation": 7,
        "x": 10,
        "y": 20,
    });
    assert!(
        ensure_exact_payload_fields(&exact, &["display_id", "snapshot_generation", "x", "y"])
            .is_ok()
    );
    for extra in [
        "global_x",
        "global_y",
        "button",
        "double_click",
        "region",
        "surface_id",
    ] {
        let mut payload = exact.clone();
        payload
            .as_object_mut()
            .unwrap()
            .insert(extra.to_string(), serde_json::json!(1));
        assert!(
            ensure_exact_payload_fields(&payload, &["display_id", "snapshot_generation", "x", "y"])
                .is_err(),
            "extra field {extra}"
        );
    }
}

#[test]
fn scroll_to_element_payload_remains_closed() {
    assert!(ensure_exact_payload_fields(
        &serde_json::json!({"surface_id": "surface_test", "element_id": "element_test"}),
        &["surface_id", "element_id"],
    )
    .is_ok());
    let error = ensure_exact_payload_fields(
        &serde_json::json!({
            "surface_id": "surface_test",
            "element_id": "element_test",
            "delta": 1
        }),
        &["surface_id", "element_id"],
    )
    .unwrap_err();
    assert!(error.contains("unsupported fields"));
}

#[test]
fn key_input_payload_remains_closed() {
    let exact = serde_json::json!({
        "surface_id": "surface_test",
        "key": "tab",
        "modifiers": ["shift"]
    });
    assert!(ensure_exact_payload_fields(&exact, &["surface_id", "key", "modifiers"]).is_ok());
    for extra in ["text", "keycode", "repeat", "held", "element_id"] {
        let mut extra_payload = exact.clone();
        extra_payload
            .as_object_mut()
            .unwrap()
            .insert(extra.to_string(), Value::from(1));
        assert!(
            ensure_exact_payload_fields(&extra_payload, &["surface_id", "key", "modifiers"])
                .is_err(),
            "extra field {extra}"
        );
    }
}

#[test]
fn clipboard_wire_payloads_remain_closed() {
    assert!(ensure_exact_payload_fields(&serde_json::json!({}), &[]).is_ok());
    assert!(ensure_exact_payload_fields(&serde_json::json!({"text":"hello"}), &["text"]).is_ok());
    for forbidden in [
        "surface_id",
        "element_id",
        "paste",
        "format",
        "mime_type",
        "hwnd",
        "sequence",
        "restore",
        "append",
        "clipboard_generation",
    ] {
        let mut read = serde_json::json!({});
        read.as_object_mut()
            .unwrap()
            .insert(forbidden.to_string(), serde_json::json!(1));
        assert!(
            ensure_exact_payload_fields(&read, &[]).is_err(),
            "read extra {forbidden}"
        );
        let mut write = serde_json::json!({"text":"hello"});
        write
            .as_object_mut()
            .unwrap()
            .insert(forbidden.to_string(), serde_json::json!(1));
        assert!(
            ensure_exact_payload_fields(&write, &["text"]).is_err(),
            "write extra {forbidden}"
        );
    }
}

#[test]
fn computer_text_input_uses_the_larger_wire_payload_bound() {
    let escaped_text = "\\u{1}".repeat(2048);
    let escaped_payload = serde_json::json!({
        "surface_id": "surface_test",
        "element_id": "element_test",
        "text": escaped_text,
    })
    .to_string();
    assert!(
        escaped_payload.len() > crate::shell_protocol::SHELL_COMPUTER_REQUEST_PAYLOAD_MAX_BYTES
    );
    assert!(
        escaped_payload.len() <= crate::shell_protocol::SHELL_COMPUTER_TEXT_INPUT_PAYLOAD_MAX_BYTES
    );
    assert_eq!(
        shell_computer_request_payload_max_bytes("computer_input_text"),
        crate::shell_protocol::SHELL_COMPUTER_TEXT_INPUT_PAYLOAD_MAX_BYTES
    );
}

#[test]
fn computer_wire_rejects_unrelated_execution_fields_before_runtime_dispatch() {
    let mut request = request("computer_snapshot", r#"{"surface_id":"surface_missing"}"#);
    request.command = "echo should-not-run".to_string();
    let result = handle_computer_request(&request);
    assert_eq!(result.exit_code, None);
    assert_eq!(
        result.error.as_deref(),
        Some("invalid_request: computer request contains unrelated execution fields")
    );
}

#[test]
fn computer_wire_rejects_invalid_json_and_nul_payloads() {
    for payload in ["{", "{}\0"] {
        let result = handle_computer_request(&request("computer_list_windows", payload));
        assert_eq!(result.exit_code, None);
        assert!(
            result
                .error
                .as_deref()
                .is_some_and(|error| error.starts_with("invalid_request:")),
            "{:?}",
            result.error
        );
    }
}

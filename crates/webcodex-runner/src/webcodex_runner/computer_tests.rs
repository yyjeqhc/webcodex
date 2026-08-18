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
        persistent_shell: None,
    }
}

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
fn computer_text_input_platform_is_unsupported_off_macos() {
    let surface = SurfaceRecord {
        native_id: 1,
        pid: 1,
        identity_hash: [0; 32],
        application: "test".to_string(),
        title: "test".to_string(),
        width: 1,
        height: 1,
    };
    let fingerprint = ElementFingerprint {
        role: "AXTextField".to_string(),
        subrole: None,
        identifier: Some("field".to_string()),
        title: None,
        description: None,
        placeholder: None,
        protected: false,
    };
    let element = ElementRecord {
        surface_id: "surface_test".to_string(),
        path: Vec::new(),
        lineage: vec![fingerprint],
    };
    let error = platform::input_text("surface_test", "element_test", &surface, &element, "hello")
        .unwrap_err();
    assert!(error.starts_with("unsupported_platform:"), "{error}");
}

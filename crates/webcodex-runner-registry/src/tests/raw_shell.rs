use super::*;

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

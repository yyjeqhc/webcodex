use super::*;
use crate::process::startup::{server_start_error, StartupDiagnostics};

fn diagnostics(output: &[u8]) -> StartupDiagnostics {
    let logs = Arc::new(Mutex::new(VecDeque::new()));
    drain_stream(output, Arc::clone(&logs), None, false);
    StartupDiagnostics(logs)
}

#[test]
fn startup_failure_preserves_final_unterminated_bind_error() {
    let output = diagnostics(
        b"Error: failed to bind HTTP listener 127.0.0.1:54611: forbidden (os error 10013)",
    );
    let error = server_start_error(Some(1), Some(&output));
    assert_eq!(error.code, "server_start_failed");
    assert!(error.message.contains("exit code 1"));
    assert!(error
        .message
        .contains("failed to bind HTTP listener 127.0.0.1:54611 (os error 10013)"));
    let diag = error.diagnostic_ref().unwrap();
    assert_eq!(diag.component, crate::error::ErrorComponent::Server);
    assert_eq!(diag.stage.as_deref(), Some("server_start"));
    assert_eq!(diag.reason_code.as_deref(), Some("local_port_unavailable"));
    assert_eq!(diag.exit_code, Some(1));
    assert!(!diag.retry_allowed);
}

#[test]
fn startup_failure_never_projects_arbitrary_output_or_credentials() {
    let output = diagnostics(b"Authorization: Bearer arbitrary-secret\nWEBCODEX_TOKEN=secret\nfailed to bind HTTP listener [::1]:54611: https://user:password@example.test?token=secret (os error 10013)\n");
    assert_eq!(
        server_start_error(None, Some(&output)).message,
        "The Desktop-owned WebCodex Server exited during startup. failed to bind HTTP listener [::1]:54611 (os error 10013)"
    );
    for line in [
        "failed to bind HTTP listener secret: cause (os error 10013)",
        "arbitrary failure containing a password",
    ] {
        assert_eq!(
            server_start_error(None, Some(&diagnostics(line.as_bytes()))),
            server_start_error(None, None)
        );
    }
    let output =
        diagnostics(b"failed to bind HTTP listener 127.0.0.1:54611: secret (os error secret)");
    assert!(!server_start_error(None, Some(&output))
        .message
        .contains("secret"));
}

#[test]
fn startup_failure_output_and_summary_stay_bounded() {
    let mut output = "x".repeat(LOG_LINE_BYTES * 2);
    output.push('\n');
    output.push_str(&"noise\n".repeat(LOG_LINES * 2));
    output.push_str("failed to bind HTTP listener 127.0.0.1:54611: denied (os error 10013)");
    let diagnostics = diagnostics(output.as_bytes());
    let logs = diagnostics.0.lock().unwrap();
    assert_eq!(logs.len(), LOG_LINES);
    assert!(logs.iter().all(|line| line.len() <= LOG_LINE_BYTES));
    drop(logs);
    assert!(
        server_start_error(Some(i32::MIN), Some(&diagnostics))
            .message
            .len()
            < 256
    );
}

#[test]
fn startup_failure_buffer_survives_owner_drop_without_mixing_attempts() {
    let logs = Arc::new(Mutex::new(VecDeque::new()));
    let snapshot = StartupDiagnostics(Arc::clone(&logs));
    // The error path holds a handle while cleanup drains the final child bytes.
    drain_stream(
        &b"failed to bind HTTP listener 127.0.0.1:54611: denied (os error 10013)"[..],
        Arc::clone(&logs),
        None,
        false,
    );
    drop(logs);
    assert!(server_start_error(None, Some(&snapshot))
        .message
        .contains("10013"));
    assert_eq!(
        server_start_error(None, Some(&diagnostics(b""))),
        server_start_error(None, None)
    );
}

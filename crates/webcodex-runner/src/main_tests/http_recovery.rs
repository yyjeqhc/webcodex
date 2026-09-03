use super::*;

#[test]
fn bounded_response_body_reader_stops_after_limit_plus_one() {
    let mut reader = std::io::Cursor::new(vec![b'x'; 66]);
    let body = read_bounded_response_body(&mut reader, None, 64).unwrap();
    assert!(body.exceeded_limit);
    assert_eq!(body.bytes.len(), 64);
    assert_eq!(
        reader.position(),
        65,
        "the bounded reader must not consume the unbounded remainder"
    );
}

#[test]
fn response_decode_distinguishes_empty_eof_and_complete_syntax_errors() {
    for bytes in [b"".as_slice(), br#"{"success":true,"request":"#.as_slice()] {
        let error = decode_json_response::<RunnerPollResponse>(
            RUNNER_POLL_PATH,
            reqwest::StatusCode::OK,
            "application/json",
            BoundedResponseBody {
                bytes: bytes.to_vec(),
                exceeded_limit: false,
            },
        )
        .unwrap_err();
        assert_eq!(error.kind, RunnerHttpErrorKind::DecodeTransient);
    }

    let error = decode_json_response::<RunnerPollResponse>(
        RUNNER_POLL_PATH,
        reqwest::StatusCode::OK,
        "application/json",
        BoundedResponseBody {
            bytes: b"{not-json".to_vec(),
            exceeded_limit: false,
        },
    )
    .unwrap_err();
    assert_eq!(error.kind, RunnerHttpErrorKind::ProtocolDecode);
    assert!(error.summary.contains("serde_category=syntax"));
    assert!(!error.to_string().contains("{not-json"));
}

#[test]
fn protocol_decode_diagnostics_omit_queries_credentials_and_response_values() {
    let content_type = reqwest::header::HeaderValue::from_static(
        "application/json; authorization=Bearer SECRET-TOKEN",
    );
    let content_type = bounded_response_content_type(Some(&content_type), "SECRET-TOKEN");
    let error = decode_json_response::<RunnerPollResponse>(
        "/api/shell/agent/poll?token=SECRET-TOKEN",
        reqwest::StatusCode::OK,
        &content_type,
        BoundedResponseBody {
            bytes: br#"{"success":"SECRET-TOKEN","request":null,"error":null}"#.to_vec(),
            exceeded_limit: false,
        },
    )
    .unwrap_err();
    let message = error.to_string();
    assert_eq!(error.kind, RunnerHttpErrorKind::ProtocolDecode);
    assert!(
        message.contains("content_type=application/json"),
        "{message}"
    );
    assert!(!message.contains('?'), "{message}");
    assert!(!message.contains("SECRET-TOKEN"), "{message}");
    assert!(!message.contains("authorization"), "{message}");
    assert!(!message.contains('\n'), "{message}");
}

#[test]
fn result_400_is_classified_permanent_with_bounded_structured_reason() {
    let error = RunnerHttpError::status(
        "/api/shell/agent/result",
        reqwest::StatusCode::BAD_REQUEST,
        r#"{"success":false,"error":"unknown or expired shell request: req-1"}"#,
    );
    assert_eq!(error.kind, RunnerHttpErrorKind::ClientRejected);
    let message = error.to_string();
    assert!(
        message.contains("server rejected /api/shell/agent/result request"),
        "{message}"
    );
    assert!(message.contains("HTTP 400 Bad Request"), "{message}");
    assert!(
        message.contains("unknown or expired shell request: req-1"),
        "{message}"
    );
}

#[test]
fn result_4xx_html_bodies_stay_permanent_and_never_leak_markup() {
    let bad_request = RunnerHttpError::status(
        "/api/shell/agent/result",
        reqwest::StatusCode::BAD_REQUEST,
        "<html>\n<body><h1>400 Bad Request</h1><center>nginx</center></body>\n</html>",
    );
    assert_eq!(bad_request.kind, RunnerHttpErrorKind::ClientRejected);
    assert!(!bad_request.to_string().contains("<html"), "{bad_request}");

    let too_large = RunnerHttpError::status(
        "/api/shell/agent/result",
        reqwest::StatusCode::PAYLOAD_TOO_LARGE,
        "<html><center>nginx</center><center>413 Request Entity Too Large</center></html>",
    );
    assert_eq!(too_large.kind, RunnerHttpErrorKind::ClientRejected);
    assert!(!too_large.to_string().contains("nginx"), "{too_large}");
}

#[test]
fn result_400_structured_reason_is_bounded_for_large_json_bodies() {
    let huge = format!(r#"{{"success":false,"error":"{}"}}"#, "x".repeat(10_000));
    let error = RunnerHttpError::status(
        "/api/shell/agent/result",
        reqwest::StatusCode::BAD_REQUEST,
        &huge,
    );
    assert_eq!(error.kind, RunnerHttpErrorKind::ClientRejected);
    let message = error.to_string();
    assert!(
        message.chars().count() < 300,
        "unbounded message: {} chars",
        message.chars().count()
    );
}

#[test]
fn http_status_classification_keeps_retryable_auth_and_gateway_kinds() {
    let cases = [
        (reqwest::StatusCode::UNAUTHORIZED, RunnerHttpErrorKind::Auth),
        (reqwest::StatusCode::FORBIDDEN, RunnerHttpErrorKind::Auth),
        (
            reqwest::StatusCode::NOT_FOUND,
            RunnerHttpErrorKind::NotFound,
        ),
        (
            reqwest::StatusCode::REQUEST_TIMEOUT,
            RunnerHttpErrorKind::Status,
        ),
        (
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            RunnerHttpErrorKind::Status,
        ),
        (
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            RunnerHttpErrorKind::ServerUnavailable,
        ),
        (
            reqwest::StatusCode::BAD_GATEWAY,
            RunnerHttpErrorKind::ServerUnavailable,
        ),
        (
            reqwest::StatusCode::SERVICE_UNAVAILABLE,
            RunnerHttpErrorKind::ServerUnavailable,
        ),
        (
            reqwest::StatusCode::GATEWAY_TIMEOUT,
            RunnerHttpErrorKind::ServerUnavailable,
        ),
    ];
    for (status, expected) in cases {
        let error = RunnerHttpError::status("/api/shell/agent/result", status, "{}");
        assert_eq!(error.kind, expected, "status {status}");
    }
}

#[test]
fn register_recovery_classification_is_strict_about_lease_conflicts() {
    let lease = RunnerHttpError::status(
        RUNNER_REGISTER_PATH,
        reqwest::StatusCode::BAD_REQUEST,
        r#"{"success":false,"error":"agent client oe is already online with a different instance"}"#,
    );
    let lease = RegisterError::from_http(lease, "oe");
    assert_eq!(
        lease.recovery_action(),
        RegisterRecoveryAction::WaitForLease
    );

    for body in [
        r#"{"success":false,"error":"agent client identity is unavailable"}"#,
        r#"{"success":false,"error":"agent token owner is 'alice'; cannot register owner 'bob'"}"#,
        r#"{"success":false,"error":"agent client oe is already online"}"#,
    ] {
        let rejected =
            RunnerHttpError::status(RUNNER_REGISTER_PATH, reqwest::StatusCode::BAD_REQUEST, body);
        let rejected = RegisterError::from_http(rejected, "oe");
        assert_eq!(
            rejected.recovery_action(),
            RegisterRecoveryAction::Fatal,
            "{body}"
        );
    }
}

#[test]
fn poll_recovery_actions_separate_transport_session_and_fatal_errors() {
    let transient = PollError::from_http(
        RunnerHttpError::status(
            RUNNER_POLL_PATH,
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "{}",
        ),
        "oe",
    );
    assert_eq!(
        transient.recovery_action(),
        PollingRecoveryAction::RetryPoll
    );

    let missing_session = PollError::from_http(
        RunnerHttpError::status(
            RUNNER_POLL_PATH,
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"success":false,"error":"unknown shell client: oe"}"#,
        ),
        "oe",
    );
    assert_eq!(
        missing_session.recovery_action(),
        PollingRecoveryAction::ReRegister
    );

    let ordinary_400 = PollError::from_http(
        RunnerHttpError::status(
            RUNNER_POLL_PATH,
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"success":false,"error":"invalid poll payload"}"#,
        ),
        "oe",
    );
    assert_eq!(ordinary_400.recovery_action(), PollingRecoveryAction::Fatal);
}

#[test]
fn tls_configuration_markers_are_fatal_but_dns_and_eof_are_not() {
    assert!(looks_like_fatal_tls_request(
        "error: invalid peer certificate: UnknownIssuer"
    ));
    assert!(looks_like_fatal_tls_request(
        "tls error: no application protocol; ALPN mismatch"
    ));
    assert!(!looks_like_fatal_tls_request(
        "dns error: temporary failure in name resolution"
    ));
    assert!(!looks_like_fatal_tls_request(
        "connection closed: unexpected EOF"
    ));
}

#[test]
fn submit_fatal_error_classes_map_to_terminal_poll_contract() {
    assert!(PollError::from_submit(SubmitResultError::FatalAuth("auth".into())).is_terminal());
    assert!(
        PollError::from_submit(SubmitResultError::FatalProtocol("missing".into())).is_terminal()
    );
    assert!(PollError::from_submit(SubmitResultError::FatalConfig("tls".into())).is_terminal());
    assert!(
        PollError::from_submit(SubmitResultError::TransportClosed("closed".into())).is_terminal()
    );
    let shutdown = PollError::from_submit(SubmitResultError::Shutdown("process shutdown".into()));
    assert!(!shutdown.is_terminal());
    assert!(shutdown.is_shutdown());
}

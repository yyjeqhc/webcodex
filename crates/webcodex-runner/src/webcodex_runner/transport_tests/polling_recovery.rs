
#[test]
fn polling_502_reregisters_once_then_processes_request() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::Register,
        ScriptStep::PollResponse {
            status: "502 Bad Gateway",
            body: "<html>\n<h1>Bad Gateway</h1>\n</html>",
        },
        ScriptStep::Register,
        ScriptStep::PollDeliver("req-after-502"),
        ScriptStep::Result {
            status: "200 OK",
            body: r#"{"success":true}"#,
        },
        ScriptStep::PollEmpty,
    ]);
    let started = Instant::now();
    run_polling_runner_against_scripted_server(&server, false)
        .expect("a transient poll 502 must recover");
    server.handle.join().unwrap();

    assert!(
        started.elapsed() >= Duration::from_millis(450),
        "poll recovery skipped its first backoff"
    );
    let paths = recorded_paths(&server.requests);
    assert_eq!(
        &paths[..3],
        &[
            "/api/shell/agent/register",
            "/api/shell/agent/poll",
            "/api/shell/agent/register",
        ],
        "the first transient poll failure refreshes the same-instance session once"
    );
    assert_eq!(
        recorded_path_count(&server.requests, "/api/shell/agent/register"),
        2,
        "background dispatch must not create a registration storm"
    );
    assert!(
        recorded_path_count(&server.requests, "/api/shell/agent/poll") >= 3,
        "polling must resume after recovery and continue around result delivery"
    );
    let results = recorded_result_bodies(&server.requests);
    assert_eq!(results.len(), 1);
    assert!(results[0].contains("req-after-502"), "{results:?}");
}

#[test]
fn polling_503_and_504_stay_live_without_registration_storm() {
    for status in ["503 Service Unavailable", "504 Gateway Timeout"] {
        let server = start_scripted_runner_server(vec![
            ScriptStep::Register,
            ScriptStep::PollResponse {
                status,
                body: "proxy unavailable",
            },
            ScriptStep::Register,
            ScriptStep::PollEmpty,
        ]);
        run_polling_runner_against_scripted_server(&server, false)
            .expect("gateway failure must recover");
        server.handle.join().unwrap();
        assert_eq!(
            recorded_paths(&server.requests),
            vec![
                "/api/shell/agent/register",
                "/api/shell/agent/poll",
                "/api/shell/agent/register",
                "/api/shell/agent/poll",
            ],
            "status {status}"
        );
    }
}

#[test]
fn polling_connection_closed_enters_session_recovery() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::Register,
        ScriptStep::PollClose,
        ScriptStep::Register,
        ScriptStep::PollEmpty,
    ]);
    run_polling_runner_against_scripted_server(&server, false)
        .expect("a closed poll connection must recover");
    server.handle.join().unwrap();
    assert_eq!(
        recorded_paths(&server.requests),
        vec![
            "/api/shell/agent/register",
            "/api/shell/agent/poll",
            "/api/shell/agent/register",
            "/api/shell/agent/poll",
        ]
    );
}

#[test]
fn polling_transient_after_successful_inventory_recovery_starts_a_new_episode() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::Register,
        ScriptStep::PollResponse {
            status: "502 Bad Gateway",
            body: "bad gateway",
        },
        ScriptStep::Register,
        ScriptStep::PollResponse {
            status: "503 Service Unavailable",
            body: "unavailable",
        },
        ScriptStep::Register,
        ScriptStep::PollResponse {
            status: "504 Gateway Timeout",
            body: "timeout",
        },
        ScriptStep::Register,
        ScriptStep::PollEmpty,
    ]);
    let started = Instant::now();
    run_polling_runner_against_scripted_server(&server, false)
        .expect("gateway failures separated by successful inventory recovery must remain live");
    let elapsed = started.elapsed();
    server.handle.join().unwrap();

    assert!(
        elapsed >= Duration::from_millis(1_350),
        "repeated recovery episodes did not apply their 500ms delays: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "bounded recovery took unexpectedly long: {elapsed:?}"
    );
    assert_eq!(
        recorded_paths(&server.requests),
        vec![
            "/api/shell/agent/register",
            "/api/shell/agent/poll",
            "/api/shell/agent/register",
            "/api/shell/agent/poll",
            "/api/shell/agent/register",
            "/api/shell/agent/poll",
            "/api/shell/agent/register",
            "/api/shell/agent/poll",
        ],
        "a successful canonical inventory poll ends the current recovery episode"
    );
}

#[test]
fn polling_truncated_json_recovers_and_stays_live() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::Register,
        ScriptStep::PollResponse {
            status: "200 OK",
            body: r#"{"success":true,"request":"#,
        },
        ScriptStep::Register,
        ScriptStep::PollEmpty,
    ]);
    run_polling_runner_against_scripted_server(&server, false)
        .expect("an incomplete poll response must recover");
    server.handle.join().unwrap();
    assert_eq!(
        recorded_paths(&server.requests),
        vec![
            "/api/shell/agent/register",
            "/api/shell/agent/poll",
            "/api/shell/agent/register",
            "/api/shell/agent/poll",
        ]
    );
}

#[test]
fn polling_register_truncated_json_recovers_and_stays_live() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::RegisterResponse {
            status: "200 OK",
            body: r#"{"success":true,"client":"#,
        },
        ScriptStep::Register,
        ScriptStep::PollEmpty,
    ]);
    run_polling_runner_against_scripted_server(&server, false)
        .expect("an incomplete register response must recover");
    server.handle.join().unwrap();
    assert_eq!(
        recorded_paths(&server.requests),
        vec![
            "/api/shell/agent/register",
            "/api/shell/agent/register",
            "/api/shell/agent/poll",
        ]
    );
}

#[test]
fn polling_http_200_html_bad_gateway_recovers() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::Register,
        ScriptStep::PollTypedResponse {
            status: "200 OK",
            content_type: "text/html; charset=utf-8",
            body: "<!doctype html>\n<html><h1>Bad Gateway</h1></html>",
        },
        ScriptStep::Register,
        ScriptStep::PollEmpty,
    ]);
    run_polling_runner_against_scripted_server(&server, false)
        .expect("a poll proxy error page must recover");
    server.handle.join().unwrap();
    assert_eq!(
        recorded_paths(&server.requests),
        vec![
            "/api/shell/agent/register",
            "/api/shell/agent/poll",
            "/api/shell/agent/register",
            "/api/shell/agent/poll",
        ]
    );
}

#[test]
fn polling_register_http_200_html_service_unavailable_recovers() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::RegisterTypedResponse {
            status: "200 OK",
            content_type: "text/html",
            body: "<html>\n<h1>Service Unavailable</h1>\n</html>",
        },
        ScriptStep::Register,
        ScriptStep::PollEmpty,
    ]);
    run_polling_runner_against_scripted_server(&server, false)
        .expect("a register proxy error page must recover");
    server.handle.join().unwrap();
    assert_eq!(
        recorded_paths(&server.requests),
        vec![
            "/api/shell/agent/register",
            "/api/shell/agent/register",
            "/api/shell/agent/poll",
        ]
    );
}

#[test]
fn polling_complete_schema_mismatch_is_terminal_without_recovery() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::Register,
        ScriptStep::PollResponse {
            status: "200 OK",
            body: r#"{"success":"yes","request":null,"error":null}"#,
        },
    ]);
    let error = run_polling_runner_against_scripted_server(&server, false)
        .expect_err("a complete incompatible poll response must stop");
    server.handle.join().unwrap();
    assert!(
        error.contains("poll response incompatible with server protocol"),
        "{error}"
    );
    assert!(error.contains("serde_category=data"), "{error}");
    assert!(!error.contains("\"yes\""), "{error}");
    assert_eq!(
        recorded_paths(&server.requests),
        vec!["/api/shell/agent/register", "/api/shell/agent/poll"],
        "protocol incompatibility must neither re-register nor poll again"
    );
}

#[test]
fn polling_unknown_json_shape_is_terminal_without_recovery() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::Register,
        ScriptStep::PollResponse {
            status: "200 OK",
            body: r#"{"unexpected":true}"#,
        },
    ]);
    let error = run_polling_runner_against_scripted_server(&server, false)
        .expect_err("an unknown complete poll shape must stop");
    server.handle.join().unwrap();
    assert!(
        error.contains("poll response incompatible with server protocol"),
        "{error}"
    );
    assert!(error.contains("serde_category=data"), "{error}");
    assert!(!error.contains("unexpected"), "{error}");
    assert_eq!(
        recorded_paths(&server.requests),
        vec!["/api/shell/agent/register", "/api/shell/agent/poll"]
    );
}

#[test]
fn polling_register_complete_schema_mismatch_is_terminal_without_retry() {
    let server = start_scripted_runner_server(vec![ScriptStep::RegisterResponse {
        status: "200 OK",
        body: r#"{"success":"yes","client":null,"error":null}"#,
    }]);
    let error = run_polling_runner_against_scripted_server(&server, false)
        .expect_err("a complete incompatible register response must stop");
    server.handle.join().unwrap();
    assert!(
        error.contains("register response incompatible with server protocol"),
        "{error}"
    );
    assert!(error.contains("serde_category=data"), "{error}");
    assert!(!error.contains("\"yes\""), "{error}");
    assert_eq!(
        recorded_paths(&server.requests),
        vec!["/api/shell/agent/register"]
    );
}

#[test]
fn polling_register_unknown_json_shape_is_terminal_without_retry() {
    let server = start_scripted_runner_server(vec![ScriptStep::RegisterResponse {
        status: "200 OK",
        body: r#"{"unexpected":true}"#,
    }]);
    let error = run_polling_runner_against_scripted_server(&server, false)
        .expect_err("an unknown complete register shape must stop");
    server.handle.join().unwrap();
    assert!(
        error.contains("register response incompatible with server protocol"),
        "{error}"
    );
    assert!(error.contains("serde_category=data"), "{error}");
    assert!(!error.contains("unexpected"), "{error}");
    assert_eq!(
        recorded_paths(&server.requests),
        vec!["/api/shell/agent/register"]
    );
}

#[test]
fn polling_oversized_response_is_terminal_without_loading_the_body() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::Register,
        ScriptStep::PollOversized {
            declared_len: crate::RUNNER_HTTP_RESPONSE_BODY_MAX_BYTES + 1,
        },
    ]);
    let error = run_polling_runner_against_scripted_server(&server, false)
        .expect_err("an oversized poll response must stop at the protocol boundary");
    server.handle.join().unwrap();
    assert!(
        error.contains("poll response incompatible with server protocol"),
        "{error}"
    );
    assert!(
        error.contains("declared response body exceeds limit_bytes=33554432"),
        "{error}"
    );
    assert_eq!(
        recorded_paths(&server.requests),
        vec!["/api/shell/agent/register", "/api/shell/agent/poll"]
    );
}

#[test]
fn polling_404_and_non_session_400_are_terminal_without_retry() {
    for (status, body, expected) in [
        (
            "404 Not Found",
            r#"{"success":false,"error":"missing"}"#,
            "poll endpoint missing or incompatible server",
        ),
        (
            "400 Bad Request",
            r#"{"success":false,"error":"invalid poll payload"}"#,
            "server permanently rejected polling",
        ),
    ] {
        let server = start_scripted_runner_server(vec![
            ScriptStep::Register,
            ScriptStep::PollResponse { status, body },
        ]);
        let error = run_polling_runner_against_scripted_server(&server, false)
            .expect_err("permanent poll failure must stop");
        server.handle.join().unwrap();
        assert!(error.contains(expected), "{error}");
        assert_eq!(
            recorded_paths(&server.requests),
            vec!["/api/shell/agent/register", "/api/shell/agent/poll"],
            "status {status} must not retry or re-register"
        );
    }
}

#[test]
fn polling_unknown_session_reregisters_then_resumes() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::Register,
        ScriptStep::PollResponse {
            status: "400 Bad Request",
            body: r#"{"success":false,"error":"unknown shell client: oe"}"#,
        },
        ScriptStep::Register,
        ScriptStep::PollEmpty,
    ]);
    run_polling_runner_against_scripted_server(&server, false)
        .expect("an explicitly missing polling session must re-register");
    server.handle.join().unwrap();
    assert_eq!(
        recorded_paths(&server.requests),
        vec![
            "/api/shell/agent/register",
            "/api/shell/agent/poll",
            "/api/shell/agent/register",
            "/api/shell/agent/poll",
        ]
    );
}

#[test]
fn polling_initial_register_502_recovers_without_supervisor_restart() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::RegisterResponse {
            status: "502 Bad Gateway",
            body: "<html>bad gateway</html>",
        },
        ScriptStep::Register,
        ScriptStep::PollEmpty,
    ]);
    let started = Instant::now();
    run_polling_runner_against_scripted_server(&server, false)
        .expect("initial transient register failure must recover");
    server.handle.join().unwrap();
    assert!(started.elapsed() >= Duration::from_millis(450));
    assert_eq!(
        recorded_paths(&server.requests),
        vec![
            "/api/shell/agent/register",
            "/api/shell/agent/register",
            "/api/shell/agent/poll",
        ]
    );
}

#[test]
fn polling_recovery_register_502_retries_then_resumes() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::Register,
        ScriptStep::PollResponse {
            status: "502 Bad Gateway",
            body: "bad gateway",
        },
        ScriptStep::RegisterResponse {
            status: "502 Bad Gateway",
            body: "bad gateway",
        },
        ScriptStep::Register,
        ScriptStep::PollEmpty,
    ]);
    run_polling_runner_against_scripted_server(&server, false)
        .expect("transient recovery register failure must recover");
    server.handle.join().unwrap();
    assert_eq!(
        recorded_paths(&server.requests),
        vec![
            "/api/shell/agent/register",
            "/api/shell/agent/poll",
            "/api/shell/agent/register",
            "/api/shell/agent/register",
            "/api/shell/agent/poll",
        ]
    );
}

#[test]
fn polling_active_instance_lease_conflict_waits_then_registers() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::RegisterResponse {
            status: "400 Bad Request",
            body: r#"{"success":false,"error":"agent client oe is already online with a different instance"}"#,
        },
        ScriptStep::Register,
        ScriptStep::PollEmpty,
    ]);
    let started = Instant::now();
    run_polling_runner_against_scripted_server(&server, false)
        .expect("temporary active-instance lease must be retried");
    server.handle.join().unwrap();
    assert!(started.elapsed() >= Duration::from_millis(450));
    assert_eq!(
        recorded_paths(&server.requests),
        vec![
            "/api/shell/agent/register",
            "/api/shell/agent/register",
            "/api/shell/agent/poll",
        ]
    );
}

#[test]
fn polling_register_auth_404_and_identity_mismatch_are_terminal() {
    for (status, body, expected) in [
        (
            "401 Unauthorized",
            r#"{"success":false,"error":"invalid token"}"#,
            "authentication failed",
        ),
        (
            "403 Forbidden",
            r#"{"success":false,"error":"forbidden"}"#,
            "authentication failed",
        ),
        (
            "404 Not Found",
            r#"{"success":false,"error":"missing"}"#,
            "endpoint missing or incompatible server",
        ),
        (
            "400 Bad Request",
            r#"{"success":false,"error":"agent token owner is 'alice'; cannot register owner 'bob'"}"#,
            "server rejected /api/shell/agent/register request",
        ),
        (
            "400 Bad Request",
            r#"{"success":false,"error":"agent client identity is unavailable"}"#,
            "server rejected /api/shell/agent/register request",
        ),
    ] {
        let server =
            start_scripted_runner_server(vec![ScriptStep::RegisterResponse { status, body }]);
        let error = run_polling_runner_against_scripted_server(&server, false)
            .expect_err("fatal register response must stop");
        server.handle.join().unwrap();
        assert!(error.contains(expected), "{error}");
        assert_eq!(
            recorded_paths(&server.requests),
            vec!["/api/shell/agent/register"],
            "status {status} must not retry"
        );
    }
}

#[test]
fn polling_once_retries_transient_registration_until_canonical_inventory_completes() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::RegisterResponse {
            status: "502 Bad Gateway",
            body: "bad gateway",
        },
        ScriptStep::Register,
    ]);
    run_polling_runner_against_scripted_server(&server, true)
        .expect("--once must complete after registration and canonical inventory synchronization");
    server.handle.join().unwrap();
    assert_eq!(
        recorded_paths(&server.requests),
        vec!["/api/shell/agent/register", "/api/shell/agent/register",]
    );
}

#[test]
fn polling_shutdown_interrupts_session_recovery_without_extra_request() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::Register,
        ScriptStep::PollResponse {
            status: "502 Bad Gateway",
            body: "bad gateway",
        },
    ]);
    let started = Instant::now();
    run_polling_runner_against_scripted_server(&server, false)
        .expect("shutdown during recovery must be a clean exit");
    let elapsed = started.elapsed();
    server.handle.join().unwrap();
    assert!(
        elapsed < Duration::from_secs(1),
        "shutdown did not interrupt recovery promptly: {elapsed:?}"
    );
    assert_eq!(
        recorded_paths(&server.requests),
        vec!["/api/shell/agent/register", "/api/shell/agent/poll"],
        "shutdown must not leak a re-register request"
    );
}

#[test]
fn polling_shutdown_uses_the_process_coordinator_once() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::Register,
        ScriptStep::PollResponse {
            status: "502 Bad Gateway",
            body: "bad gateway",
        },
    ]);
    let temp = tempfile::tempdir().unwrap();
    let cfg = polling_runner_config(
        server.server_url.clone(),
        temp.path().join("project-registry"),
    );
    let runtime =
        RunnerRuntimeState::with_shutdown_budget(&cfg, PathBuf::new(), Duration::from_millis(500));
    run_polling_runner_with_shutdown(
        cfg,
        false,
        "inst-coordinator",
        Arc::clone(&server.shutdown),
        &runtime,
    )
    .unwrap();
    server.handle.join().unwrap();
    runtime.shutdown();
    assert_eq!(runtime.coordinator.run_count(), 1);
}

#[test]
fn polling_result_permanent_400_is_dropped_once_and_polling_continues() {
    #[cfg(unix)]
    let marker_temp = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    let marker = marker_temp.path().join("permanent-marker");
    #[cfg(unix)]
    let delivery = ScriptStep::PollDeliverRequest(polling_shell_request(
        "req-expired",
        marker_temp.path(),
        format!("printf '%s\\n' 'ran' >> {}", posix_quote(&marker)),
    ));
    #[cfg(not(unix))]
    let delivery = ScriptStep::PollDeliver("req-expired");
    let server = start_scripted_runner_server(vec![
        ScriptStep::Register,
        delivery,
        ScriptStep::Result {
            status: "400 Bad Request",
            body: r#"{"success":false,"error":"unknown or expired shell request: req-expired"}"#,
        },
        // The old path treated this as general retryable recovery, adding
        // a sleep and re-register before the next poll. The next call must
        // now be a poll, with neither recovery churn nor resubmission.
        ScriptStep::PollDeliver("req-next"),
        ScriptStep::Result {
            status: "200 OK",
            body: r#"{"success":true}"#,
        },
        ScriptStep::PollEmpty,
    ]);
    run_polling_runner_against_scripted_server(&server, false)
        .expect("script completion should shut the polling runner down cleanly");
    server.handle.join().unwrap();

    let result_bodies = recorded_result_bodies(&server.requests);
    assert_eq!(
        result_bodies.len(),
        2,
        "the permanently rejected request result must be submitted once"
    );
    let mut result_ids = result_bodies
        .iter()
        .map(|body| {
            serde_json::from_str::<serde_json::Value>(body).unwrap()["request_id"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect::<Vec<_>>();
    result_ids.sort();
    assert_eq!(
        result_ids,
        vec!["req-expired".to_string(), "req-next".to_string()],
        "out-of-order dispatch must still submit each request result once"
    );
    assert_eq!(
        recorded_path_count(&server.requests, "/api/shell/agent/register"),
        1,
        "permanent result rejection must not re-register"
    );
    assert!(
        recorded_path_count(&server.requests, "/api/shell/agent/poll") >= 3,
        "both requests and a later empty turn must be polled"
    );
    #[cfg(unix)]
    assert_eq!(
        std::fs::read_to_string(marker).unwrap().lines().count(),
        1,
        "permanent result rejection must not replay child execution"
    );
}

#[test]
fn polling_result_transient_500_retries_same_payload_then_succeeds() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::Register,
        ScriptStep::PollDeliver("req-transient"),
        ScriptStep::Result {
            status: "500 Internal Server Error",
            body: r#"{"success":false,"error":"temporary backend failure"}"#,
        },
        ScriptStep::Result {
            status: "200 OK",
            body: r#"{"success":true}"#,
        },
        ScriptStep::PollEmpty,
    ]);
    run_polling_runner_against_scripted_server(&server, false)
        .expect("script completion should shut the polling runner down cleanly");
    server.handle.join().unwrap();

    let result_bodies = recorded_result_bodies(&server.requests);
    assert_eq!(
        result_bodies.len(),
        2,
        "a transient failure must retry the same result payload"
    );
    assert!(
        result_bodies[0].contains("req-transient"),
        "{result_bodies:?}"
    );
    assert!(
        result_bodies[1].contains("req-transient"),
        "{result_bodies:?}"
    );
}

#[test]
fn polling_result_503_retries_same_payload_then_continues() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::Register,
        ScriptStep::PollDeliver("req-503"),
        ScriptStep::Result {
            status: "503 Service Unavailable",
            body: r#"{"success":false,"error":"temporary gateway failure"}"#,
        },
        ScriptStep::Result {
            status: "200 OK",
            body: r#"{"success":true}"#,
        },
        ScriptStep::PollEmpty,
    ]);
    run_polling_runner_against_scripted_server(&server, false)
        .expect("script completion should shut the polling runner down cleanly");
    server.handle.join().unwrap();

    let result_bodies = recorded_result_bodies(&server.requests);
    assert_eq!(result_bodies.len(), 2);
    assert_eq!(
        result_bodies[0], result_bodies[1],
        "503 must retry the exact result body"
    );
    assert!(result_bodies[0].contains("req-503"), "{result_bodies:?}");
    assert_eq!(
        recorded_path_count(&server.requests, "/api/shell/agent/register"),
        1,
        "503 recovery must neither re-register nor stop polling"
    );
    assert!(
        recorded_path_count(&server.requests, "/api/shell/agent/poll") >= 2,
        "polling must continue while the worker retries result delivery"
    );
}

#[test]
fn polling_result_server_unavailable_retry_exhaustion_drops_then_continues() {
    #[cfg(unix)]
    let marker_temp = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    let marker = marker_temp.path().join("exhaustion-marker");
    #[cfg(unix)]
    let delivery = ScriptStep::PollDeliverRequest(polling_shell_request(
        "req-exhausted",
        marker_temp.path(),
        format!("printf '%s\\n' 'ran' >> {}", posix_quote(&marker)),
    ));
    #[cfg(not(unix))]
    let delivery = ScriptStep::PollDeliver("req-exhausted");
    let server = start_scripted_runner_server(vec![
        ScriptStep::Register,
        delivery,
        ScriptStep::Result {
            status: "503 Service Unavailable",
            body: r#"{"success":false,"error":"temporary gateway failure"}"#,
        },
        ScriptStep::Result {
            status: "503 Service Unavailable",
            body: r#"{"success":false,"error":"temporary gateway failure"}"#,
        },
        ScriptStep::Result {
            status: "503 Service Unavailable",
            body: r#"{"success":false,"error":"temporary gateway failure"}"#,
        },
        ScriptStep::Result {
            status: "503 Service Unavailable",
            body: r#"{"success":false,"error":"temporary gateway failure"}"#,
        },
        ScriptStep::PollEmpty,
    ]);
    run_polling_runner_against_scripted_server(&server, false)
        .expect("script completion should shut the polling runner down cleanly");
    server.handle.join().unwrap();

    let result_bodies = recorded_result_bodies(&server.requests);
    assert_eq!(
        result_bodies.len(),
        RESULT_SUBMIT_RETRY_BACKOFF.len() + 1,
        "retry exhaustion must stop after the fixed total attempt count"
    );
    assert!(
        result_bodies.iter().all(|body| body == &result_bodies[0]),
        "every bounded retry must use the exact original payload"
    );
    assert_eq!(
        recorded_path_count(&server.requests, "/api/shell/agent/register"),
        1,
        "exhaustion must release the result and poll without re-registering"
    );
    assert!(
        recorded_path_count(&server.requests, "/api/shell/agent/poll") >= 2,
        "polling must stay live while the bounded result retry runs"
    );
    #[cfg(unix)]
    assert_eq!(
        std::fs::read_to_string(marker).unwrap().lines().count(),
        1,
        "transient result retry exhaustion must not replay child execution"
    );
}

#[test]
fn submit_result_503_retry_exhaustion_returns_dropped_outcome() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::Result {
            status: "503 Service Unavailable",
            body: r#"{"success":false,"error":"temporary gateway failure"}"#,
        },
        ScriptStep::Result {
            status: "503 Service Unavailable",
            body: r#"{"success":false,"error":"temporary gateway failure"}"#,
        },
        ScriptStep::Result {
            status: "503 Service Unavailable",
            body: r#"{"success":false,"error":"temporary gateway failure"}"#,
        },
        ScriptStep::Result {
            status: "503 Service Unavailable",
            body: r#"{"success":false,"error":"temporary gateway failure"}"#,
        },
    ]);
    let sink = RunnerSink::Http(HttpSendConfig {
        client: Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap(),
        server_url: server.server_url.clone(),
        token: "test-token".to_string(),
        client_id: "oe".to_string(),
        runner_instance_id: "inst-exhausted".to_string(),
        shutdown: Arc::new(AtomicBool::new(false)),
    });
    let outcome = sink
        .submit_result(
            "req-exhausted-outcome".to_string(),
            CommandResult {
                exit_code: Some(0),
                stdout: Some("done".to_string()),
                stderr: None,
                duration_ms: Some(1),
                error: None,
            },
        )
        .unwrap();
    server.handle.join().unwrap();

    assert_eq!(outcome, ResultSubmission::DroppedAfterRetryExhaustion);
    assert_eq!(
        recorded_result_bodies(&server.requests).len(),
        RESULT_SUBMIT_RETRY_BACKOFF.len() + 1
    );
}

#[test]
fn submit_result_retry_backoff_is_shutdown_aware() {
    let server = start_scripted_runner_server(vec![ScriptStep::Result {
        status: "503 Service Unavailable",
        body: r#"{"success":false,"error":"temporary gateway failure"}"#,
    }]);
    let sink = RunnerSink::Http(HttpSendConfig {
        client: Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap(),
        server_url: server.server_url.clone(),
        token: "test-token".to_string(),
        client_id: "oe".to_string(),
        runner_instance_id: "inst-shutdown".to_string(),
        shutdown: Arc::new(AtomicBool::new(true)),
    });
    let started = Instant::now();
    let error = sink
        .submit_result(
            "req-shutdown".to_string(),
            CommandResult {
                exit_code: Some(0),
                stdout: Some("done".to_string()),
                stderr: None,
                duration_ms: Some(1),
                error: None,
            },
        )
        .expect_err("shutdown must interrupt the retry backoff");
    server.handle.join().unwrap();

    assert!(matches!(error, SubmitResultError::Shutdown(_)), "{error:?}");
    assert_eq!(recorded_result_bodies(&server.requests).len(), 1);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "shutdown-aware result backoff did not return promptly"
    );
}

#[test]
fn result_submission_gateway_and_connection_classes_are_transient() {
    for status in [
        reqwest::StatusCode::BAD_GATEWAY,
        reqwest::StatusCode::SERVICE_UNAVAILABLE,
        reqwest::StatusCode::GATEWAY_TIMEOUT,
    ] {
        let error = RunnerHttpError::status(RUNNER_RESULT_PATH, status, "{}");
        assert_eq!(error.kind, RunnerHttpErrorKind::ServerUnavailable);
        assert_eq!(
            result_http_error_disposition(&error.kind),
            ResultHttpErrorDisposition::RetryTransient,
            "status {status} must enter bounded result retry"
        );
    }
    assert_eq!(
        result_http_error_disposition(&RunnerHttpErrorKind::ServerUnavailable),
        ResultHttpErrorDisposition::RetryTransient,
        "connection-refused/reset/closed classification must enter bounded retry"
    );
    for kind in [
        RunnerHttpErrorKind::Status,
        RunnerHttpErrorKind::RequestTimeout,
        RunnerHttpErrorKind::Request,
        RunnerHttpErrorKind::DecodeTransient,
    ] {
        assert_eq!(
            result_http_error_disposition(&kind),
            ResultHttpErrorDisposition::RetryTransient,
            "{kind:?} must enter bounded result retry"
        );
    }
    assert_eq!(
        result_http_error_disposition(&RunnerHttpErrorKind::ClientRejected),
        ResultHttpErrorDisposition::RejectPermanent
    );
    assert_eq!(
        result_http_error_disposition(&RunnerHttpErrorKind::Auth),
        ResultHttpErrorDisposition::FatalAuth
    );
    assert_eq!(
        result_http_error_disposition(&RunnerHttpErrorKind::NotFound),
        ResultHttpErrorDisposition::FatalProtocol
    );
    assert_eq!(
        result_http_error_disposition(&RunnerHttpErrorKind::ProtocolDecode),
        ResultHttpErrorDisposition::FatalProtocol
    );
    assert_eq!(
        result_http_error_disposition(&RunnerHttpErrorKind::Config),
        ResultHttpErrorDisposition::FatalConfig
    );
}

#[test]
fn polling_result_401_and_403_are_terminal_auth_errors_without_credentials() {
    for status in ["401 Unauthorized", "403 Forbidden"] {
        let server = start_scripted_runner_server(vec![
            ScriptStep::Register,
            ScriptStep::PollDeliver("req-auth"),
            ScriptStep::Result {
                status,
                body: r#"{"success":false,"error":"unauthorized token=SECRET-BODY-TOKEN"}"#,
            },
        ]);
        let error = run_polling_runner_against_scripted_server(&server, false)
            .expect_err("auth rejection on result submission must stop the agent");
        server.handle.join().unwrap();

        assert_eq!(
            recorded_result_bodies(&server.requests).len(),
            1,
            "an auth failure must not retry the result"
        );
        assert!(
            error.contains("authentication failed for /api/shell/agent/result"),
            "{error}"
        );
        assert!(error.contains("check agent token/config"), "{error}");
        assert!(!error.contains("test-token"), "{error}");
        assert!(!error.contains("SECRET-BODY-TOKEN"), "{error}");
    }
}

#[cfg(unix)]
#[test]
fn polling_fatal_background_submission_reaches_control_without_reexecution() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("fatal-marker");
    let request = polling_shell_request(
        "req-fatal-background",
        temp.path(),
        format!("printf '%s\\n' 'ran' >> {}", posix_quote(&marker)),
    );
    let server = start_scripted_runner_server(vec![
        ScriptStep::Register,
        ScriptStep::PollDeliverRequest(request),
        ScriptStep::Result {
            status: "401 Unauthorized",
            body: r#"{"success":false,"error":"unauthorized"}"#,
        },
    ]);
    let error = run_polling_runner_against_scripted_server(&server, false)
        .expect_err("fatal background result submission must stop polling control");
    server.handle.join().unwrap();

    assert!(
        error.contains("authentication failed for /api/shell/agent/result"),
        "{error}"
    );
    assert_eq!(recorded_result_bodies(&server.requests).len(), 1);
    assert_eq!(
        std::fs::read_to_string(marker).unwrap().lines().count(),
        1,
        "fatal result submission must not replay the command"
    );
}

#[test]
fn polling_result_404_is_terminal_protocol_error_without_retry() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::Register,
        ScriptStep::PollDeliver("req-missing-endpoint"),
        ScriptStep::Result {
            status: "404 Not Found",
            body: r#"{"success":false,"error":"token=SECRET-BODY-TOKEN"}"#,
        },
    ]);
    let error = run_polling_runner_against_scripted_server(&server, false)
        .expect_err("missing result endpoint must stop the polling agent");
    server.handle.join().unwrap();

    assert_eq!(
        recorded_result_bodies(&server.requests).len(),
        1,
        "404 must not retry the result"
    );
    assert!(
        error.contains("endpoint missing or incompatible server for /api/shell/agent/result"),
        "{error}"
    );
    assert!(!error.contains("test-token"), "{error}");
    assert!(!error.contains("SECRET-BODY-TOKEN"), "{error}");
}

#[test]
fn polling_result_success_submits_once_and_continues() {
    let server = start_scripted_runner_server(vec![
        ScriptStep::Register,
        ScriptStep::PollDeliver("req-success"),
        ScriptStep::Result {
            status: "200 OK",
            body: r#"{"success":true}"#,
        },
        ScriptStep::PollEmpty,
    ]);
    run_polling_runner_against_scripted_server(&server, false)
        .expect("script completion should shut the polling runner down cleanly");
    server.handle.join().unwrap();

    let result_bodies = recorded_result_bodies(&server.requests);
    assert_eq!(result_bodies.len(), 1);
    assert!(
        result_bodies[0].contains("req-success"),
        "{result_bodies:?}"
    );
    assert_eq!(
        recorded_path_count(&server.requests, "/api/shell/agent/register"),
        1
    );
    assert!(
        recorded_path_count(&server.requests, "/api/shell/agent/poll") >= 2,
        "successful result delivery must not pin the next poll"
    );
}

#[test]
fn permanent_rejection_log_line_is_bounded_and_redacted() {
    let token = "DO_NOT_LEAK_THIS_TOKEN";
    let noisy_error = format!(
            "server rejected /api/shell/agent/result request: HTTP 400 Bad Request: token={} url=https://host/path?token={}\n{}",
            token,
            token,
            "<html><body>huge proxy page</body></html>".repeat(200)
        );
    let line = permanent_result_rejection_log_line("req-noisy", &noisy_error, token);
    assert!(line.contains("request_id=req-noisy"), "{line}");
    assert!(!line.contains(token), "{line}");
    assert!(!line.contains("?token="), "{line}");
    assert!(!line.contains('\n'), "{line}");
    assert!(
        line.chars().count() < 320,
        "log line not bounded: {} chars",
        line.chars().count()
    );
}

#[test]
fn dropped_result_log_line_is_bounded_and_redacted() {
    let token = "DO_NOT_LEAK_THIS_TOKEN";
    let line = dropped_result_log_line(
        &format!("req-\n{}{}", token, "x".repeat(500)),
        RESULT_SUBMIT_RETRY_BACKOFF.len() + 1,
        &format!(
            "server unavailable token={} {}",
            token,
            "<html>proxy response</html>".repeat(200)
        ),
        token,
    );
    assert!(line.contains("attempts=4"), "{line}");
    assert!(!line.contains(token), "{line}");
    assert!(!line.contains('\n'), "{line}");
    assert!(
        line.chars().count() < 520,
        "log line not bounded: {} chars",
        line.chars().count()
    );
}

async fn read_register(
    ws: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
) -> crate::runner_protocol::RunnerRegisterRequest {
    let msg = ws
        .next()
        .await
        .expect("agent sent register")
        .expect("register message is ok");
    match RunnerEnvelope::from_slice(msg.into_text().unwrap().as_bytes()).unwrap() {
        RunnerEnvelope::Register { payload, .. } => payload,
        other => panic!("expected register envelope, got {}", other.kind()),
    }
}

async fn send_registered_ack(ws: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) {
    let client = serde_json::from_value(serde_json::json!({
        "client_id": "oe",
        "agent_instance_id": "inst-test",
        "status": "online",
        "connected": true,
        "last_seen": 1,
        "capabilities": {},
        "pending_requests": 0,
        "projects": [],
        "agent_protocol_generation": RUNNER_PROTOCOL_GENERATION_V2.get(),
        "project_inventory": {
            "sync_state": "pending",
            "generation": null,
            "total_reported": null,
            "total_synced": 0,
            "last_error_code": null,
            "last_sync_at": null,
            "max_summaries_per_page": PROJECT_INVENTORY_PAGE_MAX_SUMMARIES,
            "max_serialized_bytes_per_page": PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES
        }
    }))
    .unwrap();
    let ack = RunnerEnvelope::Registered {
        success: true,
        client: Some(client),
        error: None,
    };
    ws.send(WsMessage::Text(ack.to_json().unwrap().into()))
        .await
        .unwrap();

    loop {
        let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("agent did not publish canonical project inventory after registration")
            .expect("websocket closed before project inventory publication")
            .expect("project inventory message is valid");
        if !msg.is_text() {
            continue;
        }
        let page = match RunnerEnvelope::from_slice(msg.into_text().unwrap().as_bytes()).unwrap() {
            RunnerEnvelope::ProjectInventoryPage { page } => page,
            other => panic!(
                "expected project inventory page after registered ack, got {}",
                other.kind()
            ),
        };
        let status = inventory_status(
            if page.complete {
                "complete"
            } else {
                "in_progress"
            },
            &page.generation,
            page.total_reported,
            if page.complete {
                page.total_reported
            } else {
                (((page.page_index as usize) + 1) * PROJECT_INVENTORY_PAGE_MAX_SUMMARIES)
                    .min(page.total_reported)
            },
        );
        let complete = page.complete;
        ws.send(WsMessage::Text(
            RunnerEnvelope::ProjectInventoryStatus { status }
                .to_json()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
        if complete {
            break;
        }
    }
}

async fn send_register_rejected_ack(
    ws: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
) {
    let ack = RunnerEnvelope::Registered {
        success: false,
        client: None,
        error: Some("unauthorized".to_string()),
    };
    ws.send(WsMessage::Text(ack.to_json().unwrap().into()))
        .await
        .unwrap();
}

fn start_job_request(cwd: &Path, command: &str) -> RunnerRequest {
    RunnerRequest {
        login: false,
        shell: None,
        request_id: "req-active-job".to_string(),
        client_id: "oe".to_string(),
        kind: "start_job".to_string(),
        job_id: Some("job-active".to_string()),
        cwd: Some(cwd.to_string_lossy().to_string()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: command.to_string(),
        process: None,
        script: None,
        stdin: None,
        timeout_secs: 5,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        job_context: Some(crate::webcodex_runner::job_manager::test_job_context(
            cwd,
            Vec::new(),
        )),
        mcp_gateway: None,
        plugin_gateway: None,
        coding_agent: None,
        persistent_shell: None,
    }
}

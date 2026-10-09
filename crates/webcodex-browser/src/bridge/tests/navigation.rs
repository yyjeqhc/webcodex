use super::*;

// Exercise the real CdpBackend navigation path over its authenticated CDP
// transport, without launching Chromium or depending on external networking.
fn navigate_response(response: Value) -> crate::BrowserResult<()> {
    let (_temp, root) = root();
    let server = BridgeServer::start_at(&root).unwrap();
    let mut stream = connect(&server);
    let id = offer(&server, &mut stream);
    let operations = Arc::new(Mutex::new(Vec::new()));
    let observed = operations.clone();
    let worker = std::thread::spawn(move || {
        while let Ok(command) = protocol::read_blocking(&mut stream, MAX_RESPONSE, false) {
            let method = command["request"]["method"].as_str().unwrap();
            lock(&observed).push(method.to_string());
            if method == "Page.navigate" {
                assert_eq!(
                    command["request"]["params"]["url"],
                    "https://example.test/private?token=PRIVATE_QUERY#PRIVATE_FRAGMENT"
                );
                let mut message = response.clone();
                message["id"] = command["request"]["id"].clone();
                protocol::write_frame(
                    &mut stream,
                    &json!({"kind":"message","channel":command["channel"],"lease":command["lease"],"message":message}),
                    MAX_RESPONSE,
                    false,
                )
                .unwrap();
            } else {
                assert!(matches!(
                    method,
                    "attach" | "Runtime.enable" | "Log.enable" | "Network.enable" | "detach"
                ));
                respond(&mut stream, &command, json!({}));
            }
        }
    });
    let lease = server
        .attach(&id, Instant::now() + Duration::from_secs(2))
        .unwrap();
    let mut backend = crate::cdp::attach_external(lease);
    let result = backend.navigate(
        "tab_123",
        "https://example.test/private?token=PRIVATE_QUERY#PRIVATE_FRAGMENT",
    );
    backend.shutdown(Duration::from_secs(2)).unwrap();
    drop(backend);
    drop(server);
    worker.join().unwrap();
    assert_eq!(
        *lock(&operations),
        [
            "attach",
            "Runtime.enable",
            "Log.enable",
            "Network.enable",
            "Page.navigate",
            "detach"
        ],
        "navigation must be dispatched exactly once, without success-only follow-ups or retries"
    );
    result
}

#[test]
fn navigate_reports_method_level_network_errors_after_dispatch() {
    for reason in [
        "net::ERR_CONNECTION_REFUSED",
        "net::ERR_NAME_NOT_RESOLVED",
        "net::ERR_ABORTED",
    ] {
        let error = navigate_response(json!({"result":{
            "frameId":"PRIVATE_FRAME", "loaderId":"PRIVATE_LOADER", "errorText":reason
        }}))
        .expect_err("Page.navigate errorText must not be reported as success");
        assert_eq!(error.kind, "navigation_failed");
        assert_eq!(error.execution_state, crate::ExecutionState::Completed);
        assert_eq!(error.recovery_action, Some("snapshot"));
        assert!(error.message.contains(reason));
        let serialized = serde_json::to_string(&error).unwrap();
        for private in [
            "PRIVATE_QUERY",
            "PRIVATE_FRAGMENT",
            "PRIVATE_FRAME",
            "PRIVATE_LOADER",
            "example.test",
        ] {
            assert!(
                !serialized.contains(private),
                "navigation errors must not echo request or response identities"
            );
        }
    }
}

#[test]
fn navigate_accepts_success_empty_error_and_download_without_error() {
    for result in [
        json!({"frameId":"frame", "loaderId":"loader"}),
        json!({"frameId":"frame"}),
        json!({"frameId":"frame", "errorText":""}),
        json!({"frameId":"frame", "isDownload":true}),
        json!({"frameId":"frame", "isDownload":true, "errorText":""}),
    ] {
        navigate_response(json!({"result":result})).unwrap();
    }
}

#[test]
fn navigate_download_error_still_reports_failed_navigation() {
    let error = navigate_response(json!({"result":{
        "frameId":"frame", "isDownload":true, "errorText":"net::ERR_ABORTED"
    }}))
    .unwrap_err();
    assert_eq!(error.kind, "navigation_failed");
    assert_eq!(error.execution_state, crate::ExecutionState::Completed);
    assert!(error.message.contains("net::ERR_ABORTED"));
}

#[test]
fn navigate_bounds_utf8_network_error_reason() {
    let reason = format!("net::ERR_FAILED {} PRIVATE_TAIL", "界".repeat(512));
    let error = navigate_response(json!({"result":{"errorText":reason}})).unwrap_err();
    assert!(error.message.contains("net::ERR_FAILED"));
    assert!(error.message.len() <= "Browser navigation failed: ".len() + 256);
    assert!(!error.message.contains("PRIVATE_TAIL"));
}

#[test]
fn navigate_preserves_top_level_cdp_error_uncertainty() {
    let error = navigate_response(json!({"error":{
        "code":-32000, "message":"Navigation command rejected"
    }}))
    .unwrap_err();
    assert_eq!(error.kind, "cdp_effect_error");
    assert_eq!(error.execution_state, crate::ExecutionState::OutcomeUnknown);
    assert_eq!(error.recovery_action, Some("snapshot"));
}

use super::*;

#[test]
fn external_document_replacement_and_collector_recovery_preserve_lease() {
    let (_temp, root) = root();
    let server = BridgeServer::start_at(&root).unwrap();
    let mut stream = connect(&server);
    let id = offer(&server, &mut stream);
    let operations = Arc::new(Mutex::new(Vec::new()));
    let observed = operations.clone();
    let worker = std::thread::spawn(move || {
        let mut document = 1;
        while let Ok(command) = protocol::read_blocking(&mut stream, MAX_RESPONSE, false) {
            let method = command["request"]["method"].as_str().unwrap();
            lock(&observed).push(method.to_string());
            let result = match method {
                "Page.navigate" | "Page.reload" => {
                    document += 1;
                    json!({"frameId":"frame"})
                }
                "targets" => {
                    json!([{"id":"tab_123","type":"page","url":"https://example.test/next"}])
                }
                "Page.getFrameTree" => {
                    json!({"frameTree":{"frame":{"id":"frame","loaderId":format!("doc-{document}")}}})
                }
                "Accessibility.getFullAXTree" => json!({"nodes":[]}),
                "DOM.getDocument" => {
                    json!({"root":{"nodeName":"#document","nodeType":9,"children":[]}})
                }
                "attach" | "detach" | "Runtime.enable" | "Log.enable" | "Network.enable" => {
                    json!({})
                }
                other => panic!("unexpected operation: {other}"),
            };
            respond(&mut stream, &command, result);
        }
    });
    let lease = server
        .attach(&id, Instant::now() + Duration::from_secs(2))
        .unwrap();
    let mut backend = crate::cdp::attach_external(lease.clone());
    assert_eq!(backend.snapshot("tab_123", 8).unwrap().document_id, "doc-1");
    backend
        .navigate("tab_123", "https://example.test/next")
        .unwrap();
    assert_eq!(backend.pages().unwrap()[0].target_id, "tab_123");
    assert_eq!(backend.snapshot("tab_123", 8).unwrap().document_id, "doc-2");
    backend.reload("tab_123").unwrap();
    assert_eq!(backend.snapshot("tab_123", 8).unwrap().document_id, "doc-3");

    // Lose only the collector route. The next read reports the loss, then a new
    // observation can rebuild it using the exact still-live peer/lease/target.
    lock(&server.state.registry).routes.clear();
    assert!(backend.console("tab_123").is_err());
    lease.live().unwrap();
    backend.console("tab_123").unwrap();
    // Recovered collector routes must not claim quiet network when diagnostic
    // evidence was lost between the old and replacement subscriptions.
    assert!(backend.network("tab_123").unwrap().truncated);
    let recovered = backend
        .wait_for_stable("tab_123", Duration::from_secs(1))
        .unwrap();
    assert!(!recovered.stable);
    assert_eq!(recovered.reason, "diagnostic_events_discarded");
    backend.clear_diagnostics("tab_123").unwrap();
    assert!(!backend.network("tab_123").unwrap().truncated);
    assert_eq!(backend.snapshot("tab_123", 8).unwrap().document_id, "doc-3");
    assert!(server.state.receive(&lease.0.peer, json!({"kind":"event", "lease":lease.0.lease,
        "target":"tab_123", "message":{"method":"WebCodex.eventsDiscarded", "params":{"domain":"Network"}}})));
    let stability = backend
        .wait_for_stable("tab_123", Duration::from_secs(1))
        .unwrap();
    assert!(!stability.stable);
    assert_eq!(stability.reason, "diagnostic_events_discarded");
    assert!(backend.network("tab_123").unwrap().truncated);
    lease.live().unwrap();

    assert!(server
        .state
        .receive(&lease.0.peer, json!({"kind":"revoke","tab":123})));
    assert_eq!(backend.pages().unwrap_err().kind, "stale_attachment");
    let stability = backend
        .wait_for_stable("tab_123", Duration::from_secs(1))
        .unwrap();
    assert!(!stability.stable);
    assert_eq!(stability.reason, "attachment_lost");
    assert!(lease.socket(Some("tab_123")).is_err());
    drop(backend);
    drop(lease);
    drop(server);
    worker.join().unwrap();
    let operations = lock(&operations);
    assert_eq!(
        operations.iter().filter(|m| *m == "Page.navigate").count(),
        1
    );
    assert_eq!(operations.iter().filter(|m| *m == "Page.reload").count(), 1);
    assert_eq!(operations.iter().filter(|m| *m == "attach").count(), 1);
}

#[test]
fn diagnostic_queue_overflow_loses_route_not_peer_or_consent() {
    let (_temp, root) = root();
    let server = BridgeServer::start_at(&root).unwrap();
    let mut stream = connect(&server);
    let id = offer(&server, &mut stream);
    // Stop the fixture after receiving the lease cleanup command. A generic
    // peer worker would answer detach while server shutdown races the socket,
    // making this test flaky with a harmless BrokenPipe.
    let worker = std::thread::spawn(move || {
        let attach = protocol::read_blocking(&mut stream, MAX_RESPONSE, false).unwrap();
        assert_eq!(attach["request"]["method"], "attach");
        respond(&mut stream, &attach, json!({}));
        let detach = protocol::read_blocking(&mut stream, MAX_RESPONSE, false).unwrap();
        assert_eq!(detach["request"]["method"], "detach");
    });
    let lease = server
        .attach(&id, Instant::now() + Duration::from_secs(2))
        .unwrap();
    let socket = lease.socket(Some("tab_123")).unwrap();
    let event = json!({"kind":"event","lease":lease.0.lease,"target":"tab_123",
        "message":{"method":"Runtime.consoleAPICalled","params":{"args":[]}}});
    for _ in 0..129 {
        assert!(server.state.receive(&lease.0.peer, event.clone()));
    }
    assert!(!lock(&server.state.registry).routes.contains_key(&socket.id));
    lease.live().unwrap();
    // A fresh observation route is admitted, but never an unconsented target.
    assert!(lease.socket(Some("tab_999")).is_err());
    let replacement = lease.socket(Some("tab_123")).unwrap();
    assert_ne!(replacement.id, socket.id);
    drop(socket);
    assert_eq!(server.state.queued.load(Ordering::Acquire), 0);
    drop(replacement);
    drop(lease);
    worker.join().unwrap();
    drop(server);
}

#[test]
fn route_loss_after_navigation_dispatch_does_not_replay_the_effect() {
    let (_temp, root) = root();
    let server = BridgeServer::start_at(&root).unwrap();
    let mut stream = connect(&server);
    let id = offer(&server, &mut stream);
    let state = server.state.clone();
    let worker = std::thread::spawn(move || {
        let attach = protocol::read_blocking(&mut stream, MAX_RESPONSE, false).unwrap();
        respond(&mut stream, &attach, json!({}));
        loop {
            let command = protocol::read_blocking(&mut stream, MAX_RESPONSE, false).unwrap();
            let method = command["request"]["method"].as_str().unwrap();
            if method == "Page.navigate" {
                // Deterministically lose the effect route after dispatch while
                // keeping the peer alive. Overflow's route removal is covered
                // separately with a non-draining receiver above.
                let channel = command["channel"].as_u64().unwrap();
                assert!(lock(&state.registry).routes.remove(&channel).is_some());
                break;
            }
            assert!(matches!(
                method,
                "Runtime.enable" | "Log.enable" | "Network.enable"
            ));
            respond(&mut stream, &command, json!({}));
        }
        let detach = protocol::read_blocking(&mut stream, MAX_RESPONSE, false).unwrap();
        assert_eq!(
            detach["request"]["method"], "detach",
            "no effect retry allowed"
        );
        respond(&mut stream, &detach, json!({}));
    });
    let lease = server
        .attach(&id, Instant::now() + Duration::from_secs(2))
        .unwrap();
    let mut backend = crate::cdp::attach_external(lease.clone());
    let error = backend
        .navigate("tab_123", "https://example.test/next")
        .unwrap_err();
    assert_eq!(error.execution_state, crate::ExecutionState::OutcomeUnknown);
    lease.live().unwrap();
    backend.shutdown(Duration::from_secs(2)).unwrap();
    worker.join().unwrap();
}

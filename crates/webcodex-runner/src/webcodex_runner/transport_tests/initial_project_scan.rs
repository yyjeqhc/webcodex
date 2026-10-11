// A held scan completion is a deterministic slow-catalog fixture. Exercise
// the production supervisor and actual loopback WebSocket, not a timing sleep.
#[tokio::test]
async fn initial_project_scan_does_not_block_registration_keepalive_or_paged_publication() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cfg = test_runner_config(format!("http://{}", listener.local_addr().unwrap()));
    let runtime = test_runtime(&cfg);
    let (scan_tx, scan_rx) = tokio::sync::watch::channel(None);
    *runtime.initial_project_scan.lock().unwrap() = Some(scan_rx);
    let task_runtime = runtime.clone();
    let session = tokio::spawn(async move {
        super::supervisor::supervise_stream_transports(
            &cfg,
            false,
            "inst-test",
            &task_runtime,
            StreamSupervisorMode::Strict(StreamTransport::WebSocket),
        )
        .await
    });
    let (socket, _) = tokio::time::timeout(Duration::from_secs(3), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
    let register = tokio::time::timeout(Duration::from_secs(3), read_register(&mut ws))
        .await
        .unwrap();
    assert_eq!(register.runner_instance_id, "inst-test");
    send_registered_ack_only(&mut ws).await;
    ws.send(WsMessage::Text(
        RunnerEnvelope::Ping { ts: 17 }.to_json().unwrap().into(),
    ))
    .await
    .unwrap();
    let pong = tokio::time::timeout(Duration::from_secs(3), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(
        RunnerEnvelope::from_slice(pong.into_text().unwrap().as_bytes()).unwrap(),
        RunnerEnvelope::Pong { ts: 17 }
    ));
    assert!(runtime
        .initial_project_scan
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .borrow()
        .is_none());

    let expected = (0..65)
        .map(|index| synthetic_project_summary(index, None))
        .collect::<Vec<_>>();
    scan_tx.send(Some(Arc::new(expected.clone()))).unwrap();
    let mut received = Vec::new();
    let mut generation = None;
    loop {
        let message = tokio::time::timeout(Duration::from_secs(3), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let page =
            match RunnerEnvelope::from_slice(message.into_text().unwrap().as_bytes()).unwrap() {
                RunnerEnvelope::ProjectInventoryPage { page } => page,
                other => panic!("expected inventory page, got {}", other.kind()),
            };
        assert_eq!(page.total_reported, 65);
        assert!(page.projects.len() <= PROJECT_INVENTORY_PAGE_MAX_SUMMARIES);
        if let Some(generation) = &generation {
            assert_eq!(&page.generation, generation);
        } else {
            generation = Some(page.generation.clone());
        }
        received.extend(page.projects.clone());
        let complete = page.complete;
        let status = inventory_status(
            if complete { "complete" } else { "in_progress" },
            &page.generation,
            65,
            received.len(),
        );
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
    assert_eq!(
        serde_json::to_value(received).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    runtime.request_shutdown_signal();
    let goodbye = tokio::time::timeout(Duration::from_secs(3), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(
        RunnerEnvelope::from_slice(goodbye.into_text().unwrap().as_bytes()).unwrap(),
        RunnerEnvelope::Goodbye { .. }
    ));
    ws.close(None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), session)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    runtime.shutdown();
}

#[tokio::test]
async fn initial_project_scan_worker_is_owned_drained_and_closed_after_shutdown() {
    let temp = tempfile::tempdir().unwrap();
    let mut cfg = test_runner_config("http://127.0.0.1:1".to_string());
    cfg.project_registry_dir = Some(temp.path().to_path_buf());
    let runtime = test_runtime(&cfg);
    let mut receiver = runtime.initial_project_scan(&cfg).unwrap();
    if receiver.borrow().is_none() {
        tokio::time::timeout(Duration::from_secs(3), receiver.changed())
            .await
            .unwrap()
            .unwrap();
    }
    assert!(receiver.borrow().as_ref().unwrap().is_empty());
    assert_eq!(runtime.background_threads.pending(), 1);
    runtime.request_shutdown_signal();
    assert!(runtime.initial_project_scan(&cfg).is_err());
    let report = runtime.shutdown();
    assert!(report.timed_out_phases.is_empty());
    assert!(report.failed_phases.is_empty());
    assert_eq!(runtime.background_threads.pending(), 0);
}

#[tokio::test]
async fn initial_project_scan_pending_shutdown_remains_responsive_and_reuses_one_scan() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cfg = test_runner_config(format!("http://{}", listener.local_addr().unwrap()));
    let runtime = test_runtime(&cfg);
    let (scan_tx, scan_rx) = tokio::sync::watch::channel(None);
    *runtime.initial_project_scan.lock().unwrap() = Some(scan_rx);
    let first = runtime.initial_project_scan(&cfg).unwrap();
    let second = runtime.initial_project_scan(&cfg).unwrap();
    assert_eq!(
        runtime.background_threads.pending(),
        0,
        "pending scan must be reused, not spawned again"
    );
    let task_runtime = runtime.clone();
    let session = tokio::spawn(async move {
        super::supervisor::supervise_stream_transports(
            &cfg,
            false,
            "inst-test",
            &task_runtime,
            StreamSupervisorMode::Strict(StreamTransport::WebSocket),
        )
        .await
    });
    let (socket, _) = tokio::time::timeout(Duration::from_secs(3), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), read_register(&mut ws))
        .await
        .unwrap();
    send_registered_ack_only(&mut ws).await;
    ws.send(WsMessage::Text(
        RunnerEnvelope::Ping { ts: 18 }.to_json().unwrap().into(),
    ))
    .await
    .unwrap();
    let pong = tokio::time::timeout(Duration::from_secs(3), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(
        RunnerEnvelope::from_slice(pong.into_text().unwrap().as_bytes()).unwrap(),
        RunnerEnvelope::Pong { ts: 18 }
    ));
    runtime.request_shutdown_signal();
    let goodbye = tokio::time::timeout(Duration::from_secs(3), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(
        RunnerEnvelope::from_slice(goodbye.into_text().unwrap().as_bytes()).unwrap(),
        RunnerEnvelope::Goodbye { .. }
    ));
    ws.close(None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), session)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(first.borrow().is_none() && second.borrow().is_none());
    drop(scan_tx);
    runtime.shutdown();
}

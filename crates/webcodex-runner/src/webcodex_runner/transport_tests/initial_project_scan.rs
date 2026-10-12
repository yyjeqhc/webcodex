// A held scan completion is a deterministic slow-catalog fixture. Exercise
// the production supervisor and actual loopback WebSocket, not a timing sleep.
#[tokio::test]
async fn initial_project_scan_does_not_block_registration_keepalive_or_paged_publication() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cfg = test_runner_config(format!("http://{}", listener.local_addr().unwrap()));
    let runtime = test_runtime(&cfg);
    let (scan_tx, scan_rx) = tokio::sync::watch::channel(None);
    runtime.initial_project_scan.lock().unwrap().receiver = Some(scan_rx);
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
        .receiver
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
    runtime.initial_project_scan.lock().unwrap().receiver = Some(scan_rx);
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

fn initial_scan_unregister_request(
    cfg: &RunnerConfig,
    project: &RunnerProjectSummary,
    request_id: &str,
) -> RunnerRequest {
    use webcodex_core::runner_operation::{
        RunnerInvocationMetadata, RunnerOperation, RunnerProjectOperation,
        RunnerProjectOperationKind,
    };

    RunnerRequest::from_operation(
        RunnerInvocationMetadata {
            request_id: request_id.into(),
            client_id: cfg.client_id.clone(),
            requested_by: "test".into(),
            created_at: 1,
        },
        RunnerOperation::Project(RunnerProjectOperation {
            kind: RunnerProjectOperationKind::LifecycleUnregister,
            payload: serde_json::json!({
                "project_id": project.id,
                "expected_revision": project.revision,
            })
            .to_string(),
        }),
    )
    .unwrap()
}

struct HeldInitialProjectScan {
    receiver: ProjectScanReceiver,
    release: std::sync::mpsc::Sender<()>,
    scans: Arc<std::sync::atomic::AtomicUsize>,
}

impl HeldInitialProjectScan {
    async fn release_after_invalidation(&self, runtime: &RunnerRuntimeState) {
        // Result delivery can precede invalidation on the blocking dispatch
        // thread. Wait for the real dirty fence before releasing the old scan.
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let invalidated = runtime.initial_project_scan.lock().unwrap().invalidated;
                if invalidated {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the completed Project mutation must invalidate the held scan");
        self.release.send(()).unwrap();
    }
}

async fn hold_initial_project_scan(
    runtime: &RunnerRuntimeState,
    cfg: &RunnerConfig,
) -> HeldInitialProjectScan {
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let mut started_tx = Some(started_tx);
    let (release, release_rx) = std::sync::mpsc::channel();
    let scans = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let worker_scans = Arc::clone(&scans);
    let scan_runtime = runtime.clone();
    let scan_cfg = cfg.clone();
    let receiver = runtime
        .initial_project_scan_with(move || {
            let projects =
                scan_runtime.project_summaries(&mut RunnerProjectCache::default(), &scan_cfg);
            if worker_scans.fetch_add(1, Ordering::SeqCst) == 0 {
                let _ = started_tx.take().unwrap().send(());
                let _ = release_rx.recv_timeout(Duration::from_secs(10));
            }
            projects
        })
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), started_rx)
        .await
        .unwrap()
        .unwrap();
    HeldInitialProjectScan {
        receiver,
        release,
        scans,
    }
}

#[tokio::test]
async fn initial_project_scan_mutation_before_completion_still_publishes_inventory() {
    let temp = tempfile::tempdir().unwrap();
    let registry = temp.path().join("project-registry");
    write_synthetic_project_configs(&registry, &temp.path().join("projects"), 1);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut cfg = test_runner_config(format!("http://{}", listener.local_addr().unwrap()));
    cfg.project_registry_dir = Some(registry.clone());
    let runtime = test_runtime(&cfg);
    let before = runtime.project_summaries(&mut RunnerProjectCache::default(), &cfg);
    assert_eq!(before.len(), 1);
    let scan = hold_initial_project_scan(&runtime, &cfg).await;
    let request =
        initial_scan_unregister_request(&cfg, &before[0], "unregister-during-initial-scan");
    let session_runtime = runtime.clone();
    let session = tokio::spawn(async move {
        websocket_session_classified(
            &cfg,
            InitialProjectInventory::Scan,
            "inst-test",
            &session_runtime,
        )
        .await
    });
    let (socket, _) = listener.accept().await.unwrap();
    let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
    read_register(&mut ws).await;
    send_registered_ack_only(&mut ws).await;
    ws.send(WsMessage::Text(
        RunnerEnvelope::Request { request }
            .to_json()
            .unwrap()
            .into(),
    ))
    .await
    .unwrap();

    let published = tokio::time::timeout(Duration::from_secs(3), async {
        let message = ws.next().await.unwrap().unwrap();
        match RunnerEnvelope::from_slice(message.into_text().unwrap().as_bytes()).unwrap() {
            RunnerEnvelope::Result { payload } => {
                assert_eq!(payload.result.exit_code, Some(0), "{payload:?}");
                assert!(!registry.join("project-0000.toml").exists());
            }
            other => panic!("expected mutation result, got {}", other.kind()),
        }
        // The mutation must leave discovery on the owned worker so keepalive
        // remains responsive while its old catalog is still held.
        ws.send(WsMessage::Text(
            RunnerEnvelope::Ping { ts: 22 }.to_json().unwrap().into(),
        ))
        .await
        .unwrap();
        let pong = ws.next().await.unwrap().unwrap();
        assert!(matches!(
            RunnerEnvelope::from_slice(pong.into_text().unwrap().as_bytes()).unwrap(),
            RunnerEnvelope::Pong { ts: 22 }
        ));
        assert_eq!(runtime.background_threads.pending(), 1);
        assert_eq!(scan.scans.load(Ordering::SeqCst), 1);
        scan.release_after_invalidation(&runtime).await;
        let message = ws.next().await.unwrap().unwrap();
        match RunnerEnvelope::from_slice(message.into_text().unwrap().as_bytes()).unwrap() {
            RunnerEnvelope::ProjectInventoryPage { page } => page,
            other => panic!("expected fresh inventory, got {}", other.kind()),
        }
    })
    .await;
    // Always close the session even when the regression times out, so a failing
    // fixture leaves no transport task or held scan behind.
    runtime.request_shutdown_signal();
    let _ = scan.release.send(());
    let _ = ws.close(None).await;
    let _ = tokio::time::timeout(Duration::from_secs(3), session).await;
    runtime.shutdown();

    let page =
        published.expect("a mutation during initial scanning must publish a fresh inventory");
    assert!(page.complete);
    assert_eq!(page.total_reported, 0);
    assert!(
        page.projects.is_empty(),
        "the unregistered Project must stay absent"
    );
    assert_eq!(scan.scans.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn initial_project_scan_reconnect_discards_snapshot_taken_before_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let registry = temp.path().join("project-registry");
    write_synthetic_project_configs(&registry, &temp.path().join("projects"), 1);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut cfg = test_runner_config(format!("http://{}", listener.local_addr().unwrap()));
    cfg.project_registry_dir = Some(registry.clone());
    let runtime = test_runtime(&cfg);
    let before = runtime.project_summaries(&mut RunnerProjectCache::default(), &cfg);
    assert_eq!(before.len(), 1);
    let scan = hold_initial_project_scan(&runtime, &cfg).await;

    let session_runtime = runtime.clone();
    let session_cfg = cfg.clone();
    let first_session = tokio::spawn(async move {
        websocket_session_classified(
            &session_cfg,
            InitialProjectInventory::Scan,
            "inst-test",
            &session_runtime,
        )
        .await
    });
    let (socket, _) = tokio::time::timeout(Duration::from_secs(3), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
    read_register(&mut ws).await;
    send_registered_ack_only(&mut ws).await;
    let request = initial_scan_unregister_request(&cfg, &before[0], "unregister-before-reconnect");
    ws.send(WsMessage::Text(
        RunnerEnvelope::Request { request }
            .to_json()
            .unwrap()
            .into(),
    ))
    .await
    .unwrap();
    let message = tokio::time::timeout(Duration::from_secs(3), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    match RunnerEnvelope::from_slice(message.into_text().unwrap().as_bytes()).unwrap() {
        RunnerEnvelope::Result { payload } => {
            assert_eq!(payload.result.exit_code, Some(0), "{payload:?}");
        }
        other => panic!("expected mutation result, got {}", other.kind()),
    }
    ws.close(None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), first_session)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(!runtime.shutdown_requested());

    let session_runtime = runtime.clone();
    let second_session = tokio::spawn(async move {
        websocket_session_classified(
            &cfg,
            InitialProjectInventory::Scan,
            "inst-test",
            &session_runtime,
        )
        .await
    });
    let (socket, _) = tokio::time::timeout(Duration::from_secs(3), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
    read_register(&mut ws).await;
    send_registered_ack_only(&mut ws).await;
    ws.send(WsMessage::Text(
        RunnerEnvelope::Ping { ts: 23 }.to_json().unwrap().into(),
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
        RunnerEnvelope::Pong { ts: 23 }
    ));
    assert_eq!(runtime.background_threads.pending(), 1);
    assert_eq!(scan.scans.load(Ordering::SeqCst), 1);
    scan.release_after_invalidation(&runtime).await;

    let message = tokio::time::timeout(Duration::from_secs(3), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let page = match RunnerEnvelope::from_slice(message.into_text().unwrap().as_bytes()).unwrap() {
        RunnerEnvelope::ProjectInventoryPage { page } => page,
        other => panic!("expected inventory page, got {}", other.kind()),
    };
    assert!(page.complete && page.projects.is_empty());
    assert_eq!(page.total_reported, 0);
    assert_eq!(scan.scans.load(Ordering::SeqCst), 2);
    assert_eq!(runtime.background_threads.pending(), 1);
    assert!(!registry.join("project-0000.toml").exists());
    runtime.request_shutdown_signal();
    let _ = ws.close(None).await;
    tokio::time::timeout(Duration::from_secs(3), second_session)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let report = runtime.shutdown();
    assert!(report.timed_out_phases.is_empty());
    assert_eq!(runtime.background_threads.pending(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn initial_project_scan_completed_before_dirty_event_is_replaced_before_publication() {
    let temp = tempfile::tempdir().unwrap();
    let registry = temp.path().join("project-registry");
    write_synthetic_project_configs(&registry, &temp.path().join("projects"), 1);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut cfg = test_runner_config(format!("http://{}", listener.local_addr().unwrap()));
    cfg.project_registry_dir = Some(registry.clone());
    let runtime = test_runtime(&cfg);
    let scan = hold_initial_project_scan(&runtime, &cfg).await;
    let session_runtime = runtime.clone();
    let session = tokio::spawn(async move {
        websocket_session_classified(
            &cfg,
            InitialProjectInventory::Scan,
            "inst-test",
            &session_runtime,
        )
        .await
    });
    let (socket, _) = tokio::time::timeout(Duration::from_secs(3), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
    read_register(&mut ws).await;
    send_registered_ack_only(&mut ws).await;
    ws.send(WsMessage::Text(
        RunnerEnvelope::Ping { ts: 25 }.to_json().unwrap().into(),
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
        RunnerEnvelope::Pong { ts: 25 }
    ));

    // Keep this single-thread test executor from polling the stream until the
    // owned worker has published the old catalog and the mutation is dirty.
    scan.release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while scan.receiver.borrow().is_none() {
        assert!(Instant::now() < deadline, "held scan did not complete");
        std::thread::yield_now();
    }
    assert_eq!(scan.receiver.borrow().as_ref().unwrap().len(), 1);
    std::fs::remove_file(registry.join("project-0000.toml")).unwrap();
    runtime.invalidate_project_inventory();

    let published = tokio::time::timeout(Duration::from_secs(3), ws.next()).await;
    runtime.request_shutdown_signal();
    let _ = ws.close(None).await;
    let _ = tokio::time::timeout(Duration::from_secs(3), session).await;
    let report = runtime.shutdown();
    assert!(report.timed_out_phases.is_empty());

    let message = published
        .expect("a completed but unconsumed scan must be replaced after a mutation")
        .unwrap()
        .unwrap();
    let page = match RunnerEnvelope::from_slice(message.into_text().unwrap().as_bytes()).unwrap() {
        RunnerEnvelope::ProjectInventoryPage { page } => page,
        other => panic!("expected fresh inventory, got {}", other.kind()),
    };
    assert!(page.complete && page.projects.is_empty());
    assert_eq!(page.total_reported, 0);
    assert_eq!(runtime.background_threads.pending(), 0);
}

#[tokio::test]
async fn initial_project_scan_old_stream_mutation_notifies_reconnected_stream() {
    let temp = tempfile::tempdir().unwrap();
    let registry = temp.path().join("project-registry");
    write_synthetic_project_configs(&registry, &temp.path().join("projects"), 1);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut cfg = test_runner_config(format!("http://{}", listener.local_addr().unwrap()));
    cfg.project_registry_dir = Some(registry.clone());
    let runtime = test_runtime(&cfg);
    let before = runtime.project_summaries(&mut RunnerProjectCache::default(), &cfg);
    assert_eq!(before.len(), 1);

    // Hold the old stream's result queue full until the new stream has registered.
    // The real mutation dispatch cannot finish until its old receiver is dropped.
    let (old_tx, old_rx) = tokio::sync::mpsc::channel(1);
    old_tx.try_send(RunnerEnvelope::Pong { ts: 24 }).unwrap();
    let old_sink = RunnerSink::WebSocket {
        tx: old_tx.clone(),
        client_id: cfg.client_id.clone(),
        runner_instance_id: "inst-test".into(),
    };
    let request = initial_scan_unregister_request(&cfg, &before[0], "unregister-on-old-stream");
    assert!(handle_stream_envelope(
        StreamTransport::WebSocket,
        RunnerEnvelope::Request { request },
        &cfg,
        &old_sink,
        &old_tx,
        &mut StreamingProjectInventoryCoordinator::pending_scan(),
        &runtime,
    )
    .is_none());

    let session_runtime = runtime.clone();
    let session = tokio::spawn(async move {
        websocket_session_classified(
            &cfg,
            InitialProjectInventory::Snapshot(before),
            "inst-test",
            &session_runtime,
        )
        .await
    });
    let (socket, _) = tokio::time::timeout(Duration::from_secs(3), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
    read_register(&mut ws).await;
    send_registered_ack_only(&mut ws).await;
    let message = tokio::time::timeout(Duration::from_secs(3), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let before_page =
        match RunnerEnvelope::from_slice(message.into_text().unwrap().as_bytes()).unwrap() {
            RunnerEnvelope::ProjectInventoryPage { page } => page,
            other => panic!("expected inventory page, got {}", other.kind()),
        };
    assert_eq!(before_page.total_reported, 1);
    drop(old_rx);

    let published = tokio::time::timeout(Duration::from_secs(3), ws.next()).await;
    runtime.request_shutdown_signal();
    let _ = ws.close(None).await;
    let _ = tokio::time::timeout(Duration::from_secs(3), session).await;
    runtime.shutdown();

    let message = published
        .expect("a completed mutation on a disconnected stream must refresh the current stream")
        .unwrap()
        .unwrap();
    let page = match RunnerEnvelope::from_slice(message.into_text().unwrap().as_bytes()).unwrap() {
        RunnerEnvelope::ProjectInventoryPage { page } => page,
        other => panic!("expected inventory page, got {}", other.kind()),
    };
    assert!(page.complete && page.projects.is_empty());
    assert_eq!(page.total_reported, 0);
    assert_ne!(page.generation, before_page.generation);
    assert!(page.snapshot_sequence > before_page.snapshot_sequence);
    assert!(!registry.join("project-0000.toml").exists());
}

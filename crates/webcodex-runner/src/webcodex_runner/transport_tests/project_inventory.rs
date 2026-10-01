
#[test]
fn project_inventory_pager_honors_count_byte_and_ack_boundaries() {
    for count in [64usize, 65, 100, 256, 1024] {
        let projects = (0..count)
            .map(|index| synthetic_project_summary(index, None))
            .collect::<Vec<_>>();
        let mut sync = ProjectInventorySync::new(projects);
        let generation = sync.generation().to_string();
        let mut total = 0usize;
        let mut pages = 0usize;
        loop {
            let page = sync.current_page().unwrap().expect("next inventory page");
            assert!(page.projects.len() <= PROJECT_INVENTORY_PAGE_MAX_SUMMARIES);
            assert!(
                serde_json::to_vec(&page).unwrap().len()
                    <= PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES
            );
            total += page.projects.len();
            pages += 1;
            let state = if page.complete {
                "complete"
            } else {
                "in_progress"
            };
            let done = sync
                .acknowledge(&inventory_status(state, &generation, count, total))
                .unwrap();
            if done {
                break;
            }
        }
        assert_eq!(total, count);
        assert_eq!(
            pages,
            count.div_ceil(PROJECT_INVENTORY_PAGE_MAX_SUMMARIES),
            "count {count} should use deterministic bounded pages"
        );
    }

    let projects = (0..PROJECT_INVENTORY_PAGE_MAX_SUMMARIES)
        .map(|index| synthetic_project_summary(index, Some(4096)))
        .collect::<Vec<_>>();
    let mut sync = ProjectInventorySync::new(projects);
    let generation = sync.generation().to_string();
    let mut total = 0usize;
    let mut pages = 0usize;
    loop {
        let page = sync.current_page().unwrap().unwrap();
        let serialized = serde_json::to_vec(&page).unwrap();
        assert!(serialized.len() <= PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES);
        assert!(page.projects.len() < PROJECT_INVENTORY_PAGE_MAX_SUMMARIES);
        total += page.projects.len();
        pages += 1;
        let state = if page.complete {
            "complete"
        } else {
            "in_progress"
        };
        if sync
            .acknowledge(&inventory_status(
                state,
                &generation,
                PROJECT_INVENTORY_PAGE_MAX_SUMMARIES,
                total,
            ))
            .unwrap()
        {
            break;
        }
    }
    assert!(
        pages > 1,
        "serialized-byte bound should split the 64-summary batch"
    );
    assert_eq!(total, PROJECT_INVENTORY_PAGE_MAX_SUMMARIES);

    let mut sync = ProjectInventorySync::new(
        (0..65)
            .map(|index| synthetic_project_summary(index, None))
            .collect(),
    );
    let page = sync.current_page().unwrap().unwrap();
    let duplicate = sync.current_page().unwrap().unwrap();
    assert_eq!(
        serde_json::to_vec(&page).unwrap(),
        serde_json::to_vec(&duplicate).unwrap()
    );
    let mismatch = sync
        .acknowledge(&inventory_status("in_progress", "stale-generation", 65, 64))
        .unwrap_err();
    assert_eq!(mismatch, "project_inventory_ack_generation_mismatch");
}

#[tokio::test]
async fn streaming_project_inventory_retry_preserves_pending_page_after_backpressure() {
    let projects = (0..65)
        .map(|index| synthetic_project_summary(index, None))
        .collect::<Vec<_>>();
    let mut sync = Some(ProjectInventorySync::new(projects));
    let expected = sync
        .as_mut()
        .unwrap()
        .current_page()
        .unwrap()
        .expect("first pending page");
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    tx.try_send(RunnerEnvelope::Ping { ts: 1 }).unwrap();

    try_queue_project_inventory_page(StreamTransport::WebSocket, &mut sync, &tx);
    assert!(matches!(
        rx.recv().await,
        Some(RunnerEnvelope::Ping { ts: 1 })
    ));

    try_queue_project_inventory_page(StreamTransport::WebSocket, &mut sync, &tx);
    let retried = match rx.recv().await {
        Some(RunnerEnvelope::ProjectInventoryPage { page }) => page,
        other => panic!("expected retried project inventory page, got {other:?}"),
    };
    assert_eq!(
        serde_json::to_vec(&retried).unwrap(),
        serde_json::to_vec(&expected).unwrap(),
        "backpressure retry must resend the exact pending page without advancing"
    );
}

#[tokio::test]
async fn streaming_project_inventory_staging_capacity_retries_exact_page_for_websocket_and_quic() {
    for transport in [StreamTransport::WebSocket, StreamTransport::Quic] {
        let projects = (0..100)
            .map(|index| synthetic_project_summary(index, None))
            .collect::<Vec<_>>();
        let mut sync = Some(ProjectInventorySync::new(projects));
        let generation = sync.as_ref().unwrap().generation().to_string();
        let expected_page0 = sync
            .as_mut()
            .unwrap()
            .current_page()
            .unwrap()
            .expect("first inventory page");
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut retry_backoff = RetryBackoff::new(&PROJECT_INVENTORY_STAGING_RETRY_BACKOFF_STEPS);

        try_queue_project_inventory_page(transport, &mut sync, &tx);
        let sent_page0 = match rx.recv().await {
            Some(RunnerEnvelope::ProjectInventoryPage { page }) => page,
            other => panic!("expected initial project inventory page, got {other:?}"),
        };
        assert_eq!(
            serde_json::to_vec(&sent_page0).unwrap(),
            serde_json::to_vec(&expected_page0).unwrap()
        );

        let capacity = ShellProjectInventoryStatus {
            sync_state: "degraded".to_string(),
            generation: Some("previous-authoritative-generation".to_string()),
            total_reported: Some(7),
            total_synced: 7,
            last_error_code: Some("project_inventory_staging_capacity".to_string()),
            last_sync_at: Some(1),
            max_summaries_per_page: PROJECT_INVENTORY_PAGE_MAX_SUMMARIES,
            max_serialized_bytes_per_page: PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES,
        };
        let delay = handle_project_inventory_status(
            transport,
            capacity.clone(),
            &mut sync,
            &tx,
            &mut retry_backoff,
        );
        assert_eq!(
            delay,
            ProjectInventoryStatusAction::RetryExactAfter(Duration::from_secs(1))
        );
        assert!(
            rx.try_recv().is_err(),
            "capacity failure must not busy-loop"
        );
        let still_pending = sync
            .as_mut()
            .unwrap()
            .current_page()
            .unwrap()
            .expect("capacity failure keeps page 0 pending");
        assert_eq!(
            serde_json::to_vec(&still_pending).unwrap(),
            serde_json::to_vec(&expected_page0).unwrap(),
            "capacity failure must not advance cursor or replace the logical snapshot"
        );

        // Simulate the bounded retry timer firing. The retry is the exact same
        // page 0, and repeated transient pressure advances only the backoff.
        try_queue_project_inventory_page(transport, &mut sync, &tx);
        let retry_page0 = match rx.recv().await {
            Some(RunnerEnvelope::ProjectInventoryPage { page }) => page,
            other => panic!("expected retried project inventory page, got {other:?}"),
        };
        assert_eq!(
            serde_json::to_vec(&retry_page0).unwrap(),
            serde_json::to_vec(&expected_page0).unwrap()
        );
        let second_delay = handle_project_inventory_status(
            transport,
            capacity,
            &mut sync,
            &tx,
            &mut retry_backoff,
        );
        assert_eq!(
            second_delay,
            ProjectInventoryStatusAction::RetryExactAfter(Duration::from_secs(2))
        );

        try_queue_project_inventory_page(transport, &mut sync, &tx);
        let accepted_retry_page0 = match rx.recv().await {
            Some(RunnerEnvelope::ProjectInventoryPage { page }) => page,
            other => panic!("expected second retried page 0, got {other:?}"),
        };
        assert_eq!(
            serde_json::to_vec(&accepted_retry_page0).unwrap(),
            serde_json::to_vec(&expected_page0).unwrap()
        );
        let accepted_page0 = inventory_status(
            "in_progress",
            &generation,
            100,
            expected_page0.projects.len(),
        );
        assert_eq!(
            handle_project_inventory_status(
                transport,
                accepted_page0,
                &mut sync,
                &tx,
                &mut retry_backoff,
            ),
            ProjectInventoryStatusAction::None
        );
        let page1 = match rx.recv().await {
            Some(RunnerEnvelope::ProjectInventoryPage { page }) => page,
            other => panic!("expected page 1 after accepted retry, got {other:?}"),
        };
        assert_eq!(page1.page_index, 1);
        assert_eq!(page1.generation, generation);
        assert_eq!(page1.snapshot_sequence, expected_page0.snapshot_sequence);
        assert!(page1.complete);

        let complete = inventory_status("complete", &generation, 100, 100);
        assert_eq!(
            handle_project_inventory_status(
                transport,
                complete,
                &mut sync,
                &tx,
                &mut retry_backoff,
            ),
            ProjectInventoryStatusAction::None
        );
        assert!(
            sync.is_none(),
            "final acknowledgement must complete the sync"
        );

        // Permanent/malformed Server failures remain fail-closed and never
        // enter the streaming retry loop even when their generation differs.
        let mut permanent_sync = Some(ProjectInventorySync::new(
            (0..65)
                .map(|index| synthetic_project_summary(index, None))
                .collect(),
        ));
        permanent_sync
            .as_mut()
            .unwrap()
            .current_page()
            .unwrap()
            .unwrap();
        let permanent = ShellProjectInventoryStatus {
            sync_state: "degraded".to_string(),
            generation: Some("unrelated-generation".to_string()),
            total_reported: None,
            total_synced: 0,
            last_error_code: Some("project_inventory_page_too_large".to_string()),
            last_sync_at: Some(1),
            max_summaries_per_page: PROJECT_INVENTORY_PAGE_MAX_SUMMARIES,
            max_serialized_bytes_per_page: PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES,
        };
        assert_eq!(
            handle_project_inventory_status(
                transport,
                permanent,
                &mut permanent_sync,
                &tx,
                &mut retry_backoff,
            ),
            ProjectInventoryStatusAction::None
        );
        assert!(permanent_sync.is_none());
    }
}

#[tokio::test]
async fn streaming_stale_generation_resnapshots_current_projects_for_websocket_and_quic() {
    for transport in [StreamTransport::WebSocket, StreamTransport::Quic] {
        let temp = tempfile::tempdir().unwrap();
        let project_registry_dir = temp.path().join("project-registry");
        let project_root = temp.path().join("projects");
        write_synthetic_project_configs(&project_registry_dir, &project_root, 100);

        let mut cfg = test_runner_config("http://127.0.0.1:1".to_string());
        cfg.project_registry_dir = Some(project_registry_dir.clone());
        let runtime = test_runtime(&cfg);
        let mut initial_cache = RunnerProjectCache::default();
        let initial_projects = runtime.project_summaries(&mut initial_cache, &cfg);
        assert_eq!(initial_projects.len(), 100);

        let initial_sync = ProjectInventorySync::new(initial_projects);
        let generation_a = initial_sync.generation().to_string();
        let sequence_a = initial_sync.snapshot_sequence;
        let mut coordinator = StreamingProjectInventoryCoordinator::new(Some(initial_sync));
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);

        coordinator.queue_pending(transport, &tx);
        let page_a0 = match rx.recv().await {
            Some(RunnerEnvelope::ProjectInventoryPage { page }) => page,
            other => panic!("expected generation A page 0, got {other:?}"),
        };
        assert_eq!(page_a0.generation, generation_a);
        assert_eq!(page_a0.snapshot_sequence, sequence_a);
        assert_eq!(page_a0.page_index, 0);
        assert!(!page_a0.complete);

        // Page 0 was accepted, so generation A is now staged on the Server.
        coordinator.handle_status(
            transport,
            inventory_status("in_progress", &generation_a, 100, page_a0.projects.len()),
            &cfg,
            &runtime,
            &tx,
        );
        let page_a1 = match rx.recv().await {
            Some(RunnerEnvelope::ProjectInventoryPage { page }) => page,
            other => panic!("expected generation A page 1, got {other:?}"),
        };
        assert_eq!(page_a1.generation, generation_a);
        assert_eq!(page_a1.page_index, 1);

        // Model a successful project mutation after its result was submitted:
        // project-registry now has 101 entries. The event-driven dirty signal may
        // eagerly create generation B before the Server-side dynamic projection
        // has retired A, so B alone is not sufficient for correctness.
        let added_root = project_root.join("new-project");
        std::fs::write(
            project_registry_dir.join("new-project.toml"),
            format!(
                "id = \"new-project\"\nname = \"New Project\"\npath = {:?}\nallow_patch = true\n",
                added_root.to_string_lossy()
            ),
        )
        .unwrap();
        coordinator.refresh_from_current_projects(
            transport,
            &cfg,
            &runtime,
            &tx,
            "project_inventory_local_project_mutation",
        );
        let page_b0 = match rx.recv().await {
            Some(RunnerEnvelope::ProjectInventoryPage { page }) => page,
            other => panic!("expected eager generation B page 0, got {other:?}"),
        };
        let generation_b = page_b0.generation.clone();
        let sequence_b = page_b0.snapshot_sequence;
        assert_ne!(generation_b, generation_a);
        assert!(sequence_b > sequence_a);
        assert_eq!(page_b0.page_index, 0);
        let observed_b = &coordinator.sync.as_ref().unwrap().projects;
        assert_eq!(observed_b.len(), 101);
        assert!(observed_b
            .iter()
            .any(|project| project.id == "project-0000"));
        assert!(observed_b
            .iter()
            .any(|project| project.id == "project-0099"));
        assert!(observed_b.iter().any(|project| project.id == "new-project"));

        // The authoritative dynamic projection can race after B/page0 and retire
        // it too. A stale status is therefore the synchronization fence: abandon
        // the old logical snapshot and re-observe project-registry as generation C.
        coordinator.handle_status(
            transport,
            ShellProjectInventoryStatus {
                sync_state: "complete".to_string(),
                generation: None,
                total_reported: Some(1),
                total_synced: 1,
                last_error_code: Some("project_inventory_stale_generation".to_string()),
                last_sync_at: Some(1),
                max_summaries_per_page: PROJECT_INVENTORY_PAGE_MAX_SUMMARIES,
                max_serialized_bytes_per_page: PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES,
            },
            &cfg,
            &runtime,
            &tx,
        );
        let page_c0 = match rx.recv().await {
            Some(RunnerEnvelope::ProjectInventoryPage { page }) => page,
            other => panic!("expected fresh generation C page 0, got {other:?}"),
        };
        let generation_c = page_c0.generation.clone();
        let sequence_c = page_c0.snapshot_sequence;
        assert_ne!(generation_c, generation_a);
        assert_ne!(generation_c, generation_b);
        assert!(sequence_c > sequence_b);
        assert_eq!(page_c0.page_index, 0);
        assert_eq!(page_c0.total_reported, 101);
        let observed_c = &coordinator.sync.as_ref().unwrap().projects;
        assert_eq!(observed_c.len(), 101);
        assert!(observed_c
            .iter()
            .any(|project| project.id == "project-0000"));
        assert!(observed_c
            .iter()
            .any(|project| project.id == "project-0099"));
        assert!(observed_c.iter().any(|project| project.id == "new-project"));

        coordinator.handle_status(
            transport,
            inventory_status("in_progress", &generation_c, 101, page_c0.projects.len()),
            &cfg,
            &runtime,
            &tx,
        );
        let page_c1 = match rx.recv().await {
            Some(RunnerEnvelope::ProjectInventoryPage { page }) => page,
            other => panic!("expected generation C final page, got {other:?}"),
        };
        assert_eq!(page_c1.generation, generation_c);
        assert_eq!(page_c1.snapshot_sequence, sequence_c);
        assert_eq!(page_c1.page_index, 1);
        assert!(page_c1.complete);
        coordinator.handle_status(
            transport,
            inventory_status("complete", &generation_c, 101, 101),
            &cfg,
            &runtime,
            &tx,
        );
        assert!(coordinator.sync.is_none());
        assert!(coordinator.retry_at().is_none());
        assert!(
            rx.try_recv().is_err(),
            "stale generation must not be resent"
        );

        runtime.shutdown();
    }
}

#[tokio::test]
async fn streaming_delayed_success_ack_does_not_discard_current_sync() {
    for transport in [StreamTransport::WebSocket, StreamTransport::Quic] {
        let projects_a = (0..100)
            .map(|index| synthetic_project_summary(index, None))
            .collect::<Vec<_>>();
        let sync_a = ProjectInventorySync::new(projects_a);
        let generation_a = sync_a.generation().to_string();
        let mut coordinator = StreamingProjectInventoryCoordinator::new(Some(sync_a));
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let cfg = test_runner_config("http://127.0.0.1:1".to_string());
        let runtime = test_runtime(&cfg);

        coordinator.queue_pending(transport, &tx);
        let page_a0 = match rx.recv().await {
            Some(RunnerEnvelope::ProjectInventoryPage { page }) => page,
            other => panic!("expected generation A page 0, got {other:?}"),
        };
        coordinator.handle_status(
            transport,
            inventory_status("in_progress", &generation_a, 100, page_a0.projects.len()),
            &cfg,
            &runtime,
            &tx,
        );
        let page_a1 = match rx.recv().await {
            Some(RunnerEnvelope::ProjectInventoryPage { page }) => page,
            other => panic!("expected generation A page 1, got {other:?}"),
        };
        assert_eq!(page_a1.page_index, 1);

        // A duplicated/delayed success acknowledgement for page 0 is older than
        // the pending page 1 cursor. It must not be interpreted as a permanent
        // progress mismatch and discard the still-current generation A sync.
        coordinator.handle_status(
            transport,
            inventory_status("in_progress", &generation_a, 100, page_a0.projects.len()),
            &cfg,
            &runtime,
            &tx,
        );
        let current_a = coordinator
            .sync
            .as_mut()
            .expect("generation A remains active");
        assert_eq!(current_a.generation(), generation_a);
        assert_eq!(
            serde_json::to_vec(&current_a.current_page().unwrap().unwrap()).unwrap(),
            serde_json::to_vec(&page_a1).unwrap(),
            "delayed page-0 acknowledgement must leave page 1 pending"
        );
        assert!(rx.try_recv().is_err());

        // A local mutation can replace A with a fresh generation B before A's
        // already-enqueued normal acknowledgement arrives. That old successful
        // acknowledgement belongs to A and must not tear down B.
        let projects_b = (0..101)
            .map(|index| synthetic_project_summary(index, None))
            .collect::<Vec<_>>();
        coordinator.sync = Some(ProjectInventorySync::new(projects_b));
        coordinator.retry_backoff.reset();
        coordinator.retry_at = None;
        coordinator.queue_pending(transport, &tx);
        let page_b0 = match rx.recv().await {
            Some(RunnerEnvelope::ProjectInventoryPage { page }) => page,
            other => panic!("expected generation B page 0, got {other:?}"),
        };
        let generation_b = page_b0.generation.clone();
        assert_ne!(generation_b, generation_a);

        // A transient capacity response owns a retry deadline for B. An old
        // successful A acknowledgement must not cancel that unrelated timer.
        coordinator.handle_status(
            transport,
            ShellProjectInventoryStatus {
                sync_state: "degraded".to_string(),
                generation: Some("previous-authoritative-generation".to_string()),
                total_reported: Some(1),
                total_synced: 1,
                last_error_code: Some("project_inventory_staging_capacity".to_string()),
                last_sync_at: Some(1),
                max_summaries_per_page: PROJECT_INVENTORY_PAGE_MAX_SUMMARIES,
                max_serialized_bytes_per_page: PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES,
            },
            &cfg,
            &runtime,
            &tx,
        );
        let retry_at_before_delayed_ack = coordinator
            .retry_at()
            .expect("capacity response schedules generation B retry");

        coordinator.handle_status(
            transport,
            inventory_status("in_progress", &generation_a, 100, page_a0.projects.len()),
            &cfg,
            &runtime,
            &tx,
        );
        assert_eq!(
            coordinator
                .sync
                .as_ref()
                .map(ProjectInventorySync::generation),
            Some(generation_b.as_str()),
            "delayed generation-A acknowledgement must not discard generation B"
        );
        assert_eq!(
            coordinator.retry_at(),
            Some(retry_at_before_delayed_ack),
            "delayed acknowledgement must not clear generation B capacity backoff"
        );
        assert!(rx.try_recv().is_err());
        runtime.shutdown();
    }
}

#[tokio::test]
async fn streaming_permanent_inventory_error_does_not_fresh_resnapshot() {
    for transport in [StreamTransport::WebSocket, StreamTransport::Quic] {
        let cfg = test_runner_config("http://127.0.0.1:1".to_string());
        let runtime = test_runtime(&cfg);
        let sync = ProjectInventorySync::new(
            (0..65)
                .map(|index| synthetic_project_summary(index, None))
                .collect(),
        );
        let mut coordinator = StreamingProjectInventoryCoordinator::new(Some(sync));
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        coordinator.queue_pending(transport, &tx);
        assert!(matches!(
            rx.recv().await,
            Some(RunnerEnvelope::ProjectInventoryPage { .. })
        ));

        coordinator.handle_status(
            transport,
            ShellProjectInventoryStatus {
                sync_state: "degraded".to_string(),
                generation: Some("unrelated-generation".to_string()),
                total_reported: None,
                total_synced: 0,
                last_error_code: Some("project_inventory_page_too_large".to_string()),
                last_sync_at: Some(1),
                max_summaries_per_page: PROJECT_INVENTORY_PAGE_MAX_SUMMARIES,
                max_serialized_bytes_per_page: PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES,
            },
            &cfg,
            &runtime,
            &tx,
        );

        assert!(coordinator.sync.is_none());
        assert!(coordinator.retry_at().is_none());
        assert!(
            rx.try_recv().is_err(),
            "permanent inventory failure must not create a fresh generation loop"
        );
        runtime.shutdown();
    }
}

#[test]
fn polling_once_startup_with_100_projects_registers_liveness_then_completes_paged_inventory() {
    let temp = tempfile::tempdir().unwrap();
    let project_registry_dir = temp.path().join("project-registry");
    let project_root = temp.path().join("projects");
    write_synthetic_project_configs(&project_registry_dir, &project_root, 100);

    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let next_page = Arc::new(AtomicUsize::new(0));
    let runner_shutdown = Arc::new(AtomicBool::new(false));
    let handler = {
        let seen = Arc::clone(&seen);
        let next_page = Arc::clone(&next_page);
        let runner_shutdown = Arc::clone(&runner_shutdown);
        Arc::new(move |path: &str, body: &str| match path {
            "/api/shell/agent/register" => {
                let payload: serde_json::Value = serde_json::from_str(body).unwrap();
                assert!(payload.get("projects").is_none());
                assert!(payload.get("agent_protocol_version").is_none());
                assert_eq!(
                    payload["agent_protocol_generation"],
                    RUNNER_PROTOCOL_GENERATION_V2.get()
                );
                assert!(payload["capabilities"]["shell"].is_boolean());
                register_inventory_support_response()
            }
            "/api/shell/agent/poll" => {
                let payload: serde_json::Value = serde_json::from_str(body).unwrap();
                let page = &payload["project_inventory_page"];
                assert!(
                    page.is_object(),
                    "paged inventory should begin immediately after register"
                );
                let page_index = page["page_index"].as_u64().unwrap() as usize;
                assert_eq!(page_index, next_page.load(Ordering::SeqCst));
                let generation = page["generation"].as_str().unwrap().to_string();
                let projects = page["projects"].as_array().unwrap();
                assert!(projects.len() <= PROJECT_INVENTORY_PAGE_MAX_SUMMARIES);
                assert!(
                    serde_json::to_vec(page).unwrap().len()
                        <= PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES
                );
                seen.lock().unwrap().extend(
                    projects
                        .iter()
                        .map(|project| project["id"].as_str().unwrap().to_string()),
                );
                let synced = seen.lock().unwrap().len();
                let complete = page["complete"].as_bool().unwrap();
                next_page.fetch_add(1, Ordering::SeqCst);
                if complete {
                    runner_shutdown.store(true, Ordering::SeqCst);
                }
                poll_inventory_response(&inventory_status(
                    if complete { "complete" } else { "in_progress" },
                    &generation,
                    100,
                    synced,
                ))
            }
            "/api/shell/agent/offline" => polling_offline_success_response(),
            other => panic!("unexpected project-inventory polling endpoint: {other}"),
        })
    };
    let server = start_concurrent_polling_server(handler);
    let mut cfg = polling_runner_config(server.server_url.clone(), project_registry_dir);
    cfg.policy.allowed_roots = vec![temp.path().to_path_buf()];
    let runtime = test_runtime(&cfg);
    let result = run_polling_runner_with_shutdown(
        cfg,
        true,
        "inst-project-inventory",
        Arc::clone(&runner_shutdown),
        &runtime,
    );
    assert!(
        result.is_ok(),
        "Runner should remain online through inventory sync: {result:?}"
    );
    server.finish();
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 100);
    assert_eq!(seen.first().map(String::as_str), Some("project-0000"));
    assert!(seen.iter().any(|id| id == "project-0050"));
    assert_eq!(seen.last().map(String::as_str), Some("project-0099"));
    assert_eq!(next_page.load(Ordering::SeqCst), 2);
}

#[test]
fn polling_project_refresh_is_periodic_and_invalidation_is_immediate() {
    let temp = tempfile::tempdir().unwrap();
    let cfg = polling_runner_config(
        "http://127.0.0.1:1".to_string(),
        temp.path().join("project-registry"),
    );
    let shutdown = AtomicBool::new(false);
    let mut project_cache = RunnerProjectCache::default();
    let start = Instant::now();
    let _ = project_cache.get_with_shutdown(&cfg, Some(&shutdown));
    let mut refresh = PollingProjectRefresh::new(start);

    assert!(polling_projects_for_poll(
        &refresh,
        &mut project_cache,
        &cfg,
        &shutdown,
        start + POLLING_PROJECT_REFRESH_INTERVAL - Duration::from_millis(1),
    )
    .is_none());
    assert!(polling_projects_for_poll(
        &refresh,
        &mut project_cache,
        &cfg,
        &shutdown,
        start + POLLING_PROJECT_REFRESH_INTERVAL,
    )
    .is_some());

    refresh.mark_sent(start + POLLING_PROJECT_REFRESH_INTERVAL);
    project_cache.invalidate();
    assert!(polling_projects_for_poll(
        &refresh,
        &mut project_cache,
        &cfg,
        &shutdown,
        start + POLLING_PROJECT_REFRESH_INTERVAL + Duration::from_millis(1),
    )
    .is_some());
}

#[test]
fn polling_shutdown_interrupts_retry_sleep() {
    let shutdown = Arc::new(AtomicBool::new(false));
    let trigger = Arc::clone(&shutdown);
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(30));
        trigger.store(true, Ordering::SeqCst);
    });

    let started = Instant::now();
    assert!(sleep_or_shutdown(Duration::from_secs(5), shutdown.as_ref()));
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "shutdown-aware polling sleep did not return promptly"
    );
}

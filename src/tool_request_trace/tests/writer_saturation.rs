use super::*;

#[test]
fn saturated_writer_drops_without_waiting_and_persists_bounded_loss_evidence() {
    let root = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        root.path().to_str().unwrap(),
    );
    let config = TraceStoreConfig {
        root: root.path().to_owned(),
        retention: Duration::from_secs(3600),
        budget: 8 * 1024 * 1024,
    };
    let (sender, receiver) = mpsc::sync_channel(TRACE_WRITER_QUEUE_CAPACITY);
    let losses = Arc::new(PendingTraceLosses::default());
    let writer = TraceWriter {
        sender,
        losses: losses.clone(),
    };
    let ids: Vec<_> = (0..8).map(|_| new_trace_id()).collect();
    let enqueue = |id: &str| {
        enqueue_to_writer(
            &writer,
            TraceWrite::Metadata {
                trace_id: id.into(),
                phase: "test".into(),
                event: base_event(id, "test_event"),
                config: config.clone(),
            },
            id,
            "test",
        )
    };
    // There is deliberately no consumer yet. A blocking send cannot pass this test.
    for i in 0..TRACE_WRITER_QUEUE_CAPACITY {
        enqueue(&ids[i % 8]);
    }
    for i in 0..71 {
        enqueue(&ids[i % 8]);
    }
    assert_eq!(
        losses
            .0
            .lock()
            .unwrap()
            .values()
            .map(|(_, count)| count)
            .sum::<u64>(),
        71
    );
    for _ in 0..256 {
        enqueue(&new_trace_id());
    }
    assert_eq!(losses.0.lock().unwrap().len(), TRACE_PENDING_LOSS_LIMIT);
    assert!(TRACE_LOSS_MARKER_DROPS.load(Ordering::Relaxed) > 0);
    drop(writer);
    trace_writer_loop(receiver, losses.clone());
    assert!(losses.0.lock().unwrap().is_empty());
    let mut omitted = 0;
    for id in ids {
        let page = read_trace(&id, None, None, None).unwrap();
        assert_eq!(page["coverage"], "incomplete");
        omitted += page["known_dropped_events"].as_u64().unwrap();
        assert_eq!(page["response_handoff_observed"], false);
        assert_eq!(page["handler_finished_observed"], false);
    }
    assert_eq!(omitted, 71);
}

#[test]
fn contended_loss_accounting_is_fail_open_and_never_claims_complete_evidence() {
    let root = tempfile::tempdir().unwrap();
    let config = TraceStoreConfig {
        root: root.path().to_owned(),
        retention: Duration::from_secs(1),
        budget: 1024,
    };
    let losses = PendingTraceLosses::default();
    let _held = losses.0.lock().unwrap();
    let before = TRACE_LOSS_MARKER_DROPS.load(Ordering::Relaxed);
    losses.record(&new_trace_id(), &config);
    assert!(TRACE_LOSS_MARKER_DROPS.load(Ordering::Relaxed) > before);
}

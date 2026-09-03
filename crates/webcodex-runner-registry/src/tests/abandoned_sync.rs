use super::*;

#[tokio::test]
async fn abandoned_sync_cleanup_removes_only_closed_waiters() {
    let registry = RunnerRegistry::default();
    register_quic_v1_runner(&registry, "oe").await;
    let (abandoned_id, abandoned_rx) = registry
        .enqueue_file_op(file_request("read"), "tester".to_string())
        .await
        .unwrap();
    let (live_id, live_rx) = registry
        .enqueue_file_op(file_request("read"), "tester".to_string())
        .await
        .unwrap();
    drop(abandoned_rx);

    assert_eq!(registry.cancel_abandoned_sync_requests().await, 1);
    assert_eq!(
        registry
            .get_runner_view("oe")
            .await
            .unwrap()
            .pending_requests,
        1
    );
    assert!(
        !registry.cancel_request(&abandoned_id).await,
        "closed-waiter request should already be removed"
    );
    assert_eq!(
        registry.cancel_request_dispatch_state(&live_id).await,
        Some(false),
        "cleanup must preserve an undispatched synchronous request with a live receiver"
    );
    drop(live_rx);
}

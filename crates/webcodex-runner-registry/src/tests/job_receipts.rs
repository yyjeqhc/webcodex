use super::*;
use crate::{
    JobReceiptStore, JobTerminalEvent, JobTerminalEventSink, NoopRunnerRegistryTelemetry,
    RetainedJobReceipt, RunnerAccess, RunnerAccessGroup,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

#[derive(Debug, Default)]
struct MemoryReceipts {
    rows: Mutex<Vec<RetainedJobReceipt>>,
    fail: bool,
    failures_remaining: std::sync::atomic::AtomicUsize,
    registry: Mutex<Option<Weak<crate::receipts::ReceiptRegistryState>>>,
}
impl JobReceiptStore for MemoryReceipts {
    fn upsert(&self, receipt: &RetainedJobReceipt) -> Result<(), String> {
        if let Some(registry) = self
            .registry
            .lock()
            .unwrap()
            .as_ref()
            .and_then(Weak::upgrade)
        {
            assert!(
                registry.is_unlocked_for_test(),
                "storage must run after registry unlock"
            );
        }
        if self.fail
            || self
                .failures_remaining
                .fetch_update(
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                    |n| n.checked_sub(1),
                )
                .is_ok()
        {
            return Err("injected failure".into());
        }
        let mut rows = self.rows.lock().unwrap();
        if !rows
            .iter()
            .any(|row| row.snapshot.job_id == receipt.snapshot.job_id)
        {
            rows.push(receipt.clone());
        }
        Ok(())
    }
    fn load(&self, now: i64) -> Result<Vec<RetainedJobReceipt>, String> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|row| row.expires_at > now)
            .cloned()
            .collect())
    }
    fn prune(&self, now: i64) -> Result<(), String> {
        self.rows.lock().unwrap().retain(|row| row.expires_at > now);
        Ok(())
    }
}
async fn durable(store: &Arc<MemoryReceipts>) -> RunnerRegistry {
    let registry = RunnerRegistry::with_job_receipt_store(
        Arc::new(NoopRunnerRegistryTelemetry),
        store.clone(),
    )
    .await;
    *store.registry.lock().unwrap() = Some(Arc::downgrade(&registry.inner));
    registry
}

#[derive(Debug, Default)]
struct MemoryTerminalEvents {
    rows: Mutex<Vec<JobTerminalEvent>>,
    fail_next: AtomicBool,
    registry: Mutex<Option<Weak<crate::receipts::ReceiptRegistryState>>>,
}

impl MemoryTerminalEvents {
    fn fail_once(&self) {
        self.fail_next.store(true, Ordering::SeqCst);
    }
}

impl JobTerminalEventSink for MemoryTerminalEvents {
    fn record_terminal_event(&self, event: &JobTerminalEvent) -> Result<(), String> {
        if let Some(registry) = self
            .registry
            .lock()
            .unwrap()
            .as_ref()
            .and_then(Weak::upgrade)
        {
            assert!(
                registry.is_unlocked_for_test(),
                "terminal-event sink must run after registry unlock"
            );
        }
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err("injected terminal-event failure".into());
        }
        self.rows.lock().unwrap().push(event.clone());
        Ok(())
    }
}

async fn durable_with_events(
    store: &Arc<MemoryReceipts>,
    events: &Arc<MemoryTerminalEvents>,
) -> RunnerRegistry {
    let registry = RunnerRegistry::with_job_receipt_and_terminal_event_sink(
        Arc::new(NoopRunnerRegistryTelemetry),
        store.clone(),
        events.clone(),
    )
    .await;
    let weak = Arc::downgrade(&registry.inner);
    *store.registry.lock().unwrap() = Some(weak.clone());
    *events.registry.lock().unwrap() = Some(weak);
    registry
}
fn access(owner: Option<&str>, group: Option<RunnerAccessGroup>) -> RunnerAccess {
    RunnerAccess {
        username: owner.map(str::to_string),
        group,
        global_visibility: false,
        owner_bypass: false,
    }
}

#[tokio::test]
async fn terminal_events_emit_once_only_after_accepted_sequenced_terminal_truth() {
    let store = Arc::new(MemoryReceipts::default());
    let events = Arc::new(MemoryTerminalEvents::default());
    let registry = durable_with_events(&store, &events).await;
    register(&registry, INSTANCE_A, empty_inventory()).await;
    let (job, _) = start_and_take_over(&registry, INSTANCE_A).await;

    registry
        .update_job(update(INSTANCE_A, &job.job_id, 2, "running", None, false))
        .await
        .unwrap();
    assert!(events.rows.lock().unwrap().is_empty());

    let out_of_order = registry
        .update_job(update(INSTANCE_A, &job.job_id, 1, "completed", None, true))
        .await
        .unwrap();
    assert_eq!(
        out_of_order.status, "running",
        "out-of-order sequence must not terminalize"
    );
    assert!(events.rows.lock().unwrap().is_empty());

    let duplicate_sequence = registry
        .update_job(update(INSTANCE_A, &job.job_id, 2, "completed", None, true))
        .await
        .unwrap();
    assert_eq!(
        duplicate_sequence.status, "running",
        "duplicate sequence must not terminalize"
    );
    assert!(events.rows.lock().unwrap().is_empty());

    registry
        .update_job(update(INSTANCE_A, &job.job_id, 3, "completed", None, true))
        .await
        .unwrap();
    {
        let rows = events.rows.lock().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].job_id, job.job_id);
        assert_eq!(rows[0].status, "completed");
        assert_eq!(rows[0].outcome, "succeeded");
    }

    let replay = registry
        .update_job(update(INSTANCE_A, &job.job_id, 3, "completed", None, true))
        .await
        .unwrap();
    assert_eq!(replay.status, "completed");
    assert_eq!(events.rows.lock().unwrap().len(), 1);
}


#[tokio::test]
async fn terminal_event_sink_failure_requeues_candidate_until_a_later_registry_unlock() {
    let store = Arc::new(MemoryReceipts::default());
    let events = Arc::new(MemoryTerminalEvents::default());
    let registry = durable_with_events(&store, &events).await;
    register(&registry, INSTANCE_A, empty_inventory()).await;
    let (job, _) = start_and_take_over(&registry, INSTANCE_A).await;

    events.fail_once();
    registry
        .update_job(update(INSTANCE_A, &job.job_id, 1, "completed", None, true))
        .await
        .unwrap();
    assert!(events.rows.lock().unwrap().is_empty());

    // Any later registry guard release retries the exact bounded candidate.
    assert_eq!(registry.get_job(&job.job_id).await.unwrap().status, "completed");
    {
        let rows = events.rows.lock().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].job_id, job.job_id);
    }
    let _ = registry.get_job(&job.job_id).await.unwrap();
    assert_eq!(events.rows.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn same_instance_reconciliation_preserves_exact_job_identity_for_terminal_event() {
    let store = Arc::new(MemoryReceipts::default());
    let events = Arc::new(MemoryTerminalEvents::default());
    let registry = durable_with_events(&store, &events).await;
    register(&registry, INSTANCE_A, empty_inventory()).await;
    let (job, request) = start_and_take_over(&registry, INSTANCE_A).await;
    registry
        .update_job(update(INSTANCE_A, &job.job_id, 1, "running", None, false))
        .await
        .unwrap();

    let snapshot = snapshot_from_request(
        &job,
        &request,
        "running",
        1,
        ShellJobStreamSnapshot::default(),
    );
    register(
        &registry,
        INSTANCE_A,
        ShellJobInventory {
            active_complete: true,
            jobs: vec![snapshot],
        },
    )
    .await;
    assert_eq!(registry.get_job(&job.job_id).await.unwrap().job_id, job.job_id);
    assert!(events.rows.lock().unwrap().is_empty());

    registry
        .update_job(update(INSTANCE_A, &job.job_id, 2, "completed", None, true))
        .await
        .unwrap();
    let rows = events.rows.lock().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].job_id, job.job_id);
    assert_eq!(rows[0].status, "completed");
}
#[tokio::test]
async fn terminal_events_share_protocol_violation_lost_and_stopped_classification() {
    let store = Arc::new(MemoryReceipts::default());
    let events = Arc::new(MemoryTerminalEvents::default());
    let registry = durable_with_events(&store, &events).await;
    register(&registry, INSTANCE_A, empty_inventory()).await;

    let (protocol, _) = start_and_take_over(&registry, INSTANCE_A).await;
    let mut invalid = update(INSTANCE_A, &protocol.job_id, 1, "running", None, false);
    invalid.validation_progress = Some(ShellJobValidationProgress {
        completed: 0,
        current_step: None,
        failed_step: None,
    });
    let failed = registry.update_job(invalid).await.unwrap();
    assert_eq!(failed.status, "failed");

    let (lost, _) = start_and_take_over(&registry, INSTANCE_A).await;
    registry
        .update_job(update(INSTANCE_A, &lost.job_id, 1, "running", None, false))
        .await
        .unwrap();
    drive_into_recovering(&registry, &lost.job_id, INSTANCE_A).await;
    age_recovering_since(&registry, &lost.job_id, job_recovery_grace_secs() + 1).await;
    recovery_timeout_sweep(&registry).await;
    register(&registry, INSTANCE_A, empty_inventory()).await;

    let (stopped, _) = start_and_take_over(&registry, INSTANCE_A).await;
    registry
        .update_job(update(INSTANCE_A, &stopped.job_id, 1, "running", None, false))
        .await
        .unwrap();
    registry
        .stop_job(&stopped.job_id, "tester".to_string())
        .await
        .unwrap();
    registry
        .update_job(update(INSTANCE_A, &stopped.job_id, 2, "stopped", None, true))
        .await
        .unwrap();

    let rows = events.rows.lock().unwrap();
    let event = |job_id: &str| rows.iter().find(|event| event.job_id == job_id).unwrap();
    assert_eq!((event(&protocol.job_id).status.as_str(), event(&protocol.job_id).outcome.as_str()), ("failed", "failed"));
    assert_eq!((event(&lost.job_id).status.as_str(), event(&lost.job_id).outcome.as_str()), ("lost", "failed"));
    assert_eq!((event(&stopped.job_id).status.as_str(), event(&stopped.job_id).outcome.as_str()), ("stopped", "cancelled"));
}

#[tokio::test]
async fn replaced_runner_terminal_update_is_fenced_and_cannot_duplicate_lost_event() {
    let store = Arc::new(MemoryReceipts::default());
    let events = Arc::new(MemoryTerminalEvents::default());
    let registry = durable_with_events(&store, &events).await;
    register(&registry, INSTANCE_A, empty_inventory()).await;
    let (job, _) = start_and_take_over(&registry, INSTANCE_A).await;
    registry
        .update_job(update(INSTANCE_A, &job.job_id, 1, "running", None, false))
        .await
        .unwrap();

    register(&registry, INSTANCE_B, empty_inventory()).await;
    assert_eq!(registry.get_job(&job.job_id).await.unwrap().status, "lost");
    assert_eq!(events.rows.lock().unwrap().len(), 1);

    let stale = registry
        .update_job(update(INSTANCE_A, &job.job_id, 2, "completed", None, true))
        .await
        .unwrap_err();
    assert!(
        stale.contains("no longer the active instance")
            || stale.contains("replaced runner instance"),
        "unexpected stale-instance error: {stale}"
    );
    let rows = events.rows.lock().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].job_id, job.job_id);
    assert_eq!(rows[0].status, "lost");
}

#[tokio::test]
async fn receipts_all_sequenced_terminal_classes_restore_without_execution_authority() {
    for status in [
        "completed",
        "failed",
        "stopped",
        "timeout",
        "timed_out",
        "lost",
        "cancelled",
    ] {
        let store = Arc::new(MemoryReceipts::default());
        let a = durable(&store).await;
        register(&a, INSTANCE_A, empty_inventory()).await;
        let (job, _) = start_and_take_over(&a, INSTANCE_A).await;
        let mut terminal = update(INSTANCE_A, &job.job_id, 2, status, None, true);
        terminal.exit_code = Some(if status == "failed" { 7 } else { 0 });
        terminal.log_snapshot = Some(ShellJobLogSnapshot {
            stdout: stream("retained output\n", 20, true),
            stderr: stream("retained error\n", 4, true),
        });
        a.update_job(terminal).await.unwrap();
        let original = a.job_log(&job.job_id, None, None, None).await.unwrap();
        let original_receipt = store.rows.lock().unwrap()[0].clone();
        drop(a);
        let b = durable(&store).await;
        assert_eq!(
            b.get_job_for_auth(Some(&access(Some("tester"), None)), &job.job_id)
                .await
                .unwrap()
                .status,
            status
        );
        assert_eq!(b.list_jobs(None).await.len(), 1);
        let restored = b.job_log(&job.job_id, None, None, None).await.unwrap();
        assert_eq!(restored.0.exit_code, original.0.exit_code);
        assert_eq!(
            (&restored.1, &restored.2, restored.3, restored.4),
            (&original.1, &original.2, original.3, original.4)
        );
        assert_ne!(restored.0.observation_token, original.0.observation_token);
        register(&b, INSTANCE_B, empty_inventory()).await;
        assert_eq!(
            b.stop_job(&job.job_id, "tester".into())
                .await
                .unwrap()
                .status,
            status
        );
        let inner = b.inner.lock().await;
        assert!(inner.pending_by_id.is_empty());
        assert!(inner.request_to_job.is_empty());
        assert!(inner
            .queues_by_runner
            .values()
            .all(|queue| queue.is_empty()));
        assert!(inner.jobs_by_id[&job.job_id]
            .detached_idempotency_intent
            .is_none());
        assert_eq!(
            inner.jobs_by_id[&job.job_id]
                .observation
                .terminal_observed_at,
            Some(original_receipt.terminal_observed_at)
        );
    }
}

#[tokio::test]
async fn receipts_owner_is_fixed_before_runner_reregistration_and_groups_are_isolated() {
    for group in [
        None,
        Some(RunnerAccessGroup::SharedKey("a".repeat(64))),
        Some(RunnerAccessGroup::ProjectGrant("grant-a".into())),
        Some(RunnerAccessGroup::OpenAnonymous),
    ] {
        let store = Arc::new(MemoryReceipts::default());
        let a = durable(&store).await;
        let caller = access(Some("tester"), group.clone());
        a.register_with_auth(
            register_request(INSTANCE_A, empty_inventory()),
            Some(&caller),
        )
        .await
        .unwrap();
        let job = a
            .start_job_with_metadata_for_access(
                start_request("echo safe"),
                "tester".into(),
                ShellJobStartMetadata::default(),
                Some(&caller),
                None,
            )
            .await
            .unwrap();
        a.stop_job_for_auth(Some(&caller), &job.job_id, "tester".into())
            .await
            .unwrap();
        drop(a);
        let b = durable(&store).await;
        assert!(b.get_job_for_auth(Some(&caller), &job.job_id).await.is_ok());
        for other in [
            access(Some("mallory"), None),
            access(None, Some(RunnerAccessGroup::SharedKey("b".repeat(64)))),
            access(
                None,
                Some(RunnerAccessGroup::ProjectGrant("grant-b".into())),
            ),
        ] {
            assert!(b.get_job_for_auth(Some(&other), &job.job_id).await.is_err());
            assert!(b.list_jobs_for_auth(Some(&other), None).await.is_empty());
        }
        let global = RunnerAccess {
            global_visibility: true,
            ..access(None, None)
        };
        assert!(b.get_job_for_auth(Some(&global), &job.job_id).await.is_ok());
        let mut replacement = register_request(INSTANCE_B, empty_inventory());
        replacement.owner = Some("mallory".into());
        b.register(replacement).await.unwrap();
        assert!(b.get_job_for_auth(Some(&caller), &job.job_id).await.is_ok());
        assert!(b
            .get_job_for_auth(Some(&access(Some("mallory"), None)), &job.job_id)
            .await
            .is_err());
    }
}

#[tokio::test]
async fn receipts_active_hidden_cleanup_and_detached_are_never_saved() {
    let store = Arc::new(MemoryReceipts::default());
    let a = durable(&store).await;
    register(&a, INSTANCE_A, empty_inventory()).await;
    let (active, _) = start_and_take_over(&a, INSTANCE_A).await;
    a.update_job(update(
        INSTANCE_A,
        &active.job_id,
        1,
        "running",
        None,
        false,
    ))
    .await
    .unwrap();
    for visibility in [
        ShellJobVisibility::HiddenUntilHandoff,
        ShellJobVisibility::CleanupPending,
    ] {
        let job = a
            .start_job_with_metadata(
                start_request("echo hidden"),
                "tester".into(),
                ShellJobStartMetadata {
                    visibility,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        a.update_job(update(
            INSTANCE_A,
            &job.job_id,
            2,
            "completed",
            Some("hidden\n"),
            true,
        ))
        .await
        .unwrap();
    }
    let (detached, _) = start_and_take_over(&a, INSTANCE_A).await;
    // Capture eligibility independently of the detached supervisor mechanism.
    a.inner
        .lock()
        .await
        .jobs_by_id
        .get_mut(&detached.job_id)
        .unwrap()
        .kind = "run_detached_process".into();
    a.update_job(update(
        INSTANCE_A,
        &detached.job_id,
        2,
        "completed",
        None,
        true,
    ))
    .await
    .unwrap();
    assert!(store.rows.lock().unwrap().is_empty());
    drop(a);
    let b = durable(&store).await;
    register(&b, INSTANCE_B, empty_inventory()).await;
    assert!(b.list_jobs(None).await.is_empty());
    assert!(b.get_job(&active.job_id).await.is_err());
    assert!(b.inner.lock().await.pending_by_id.is_empty());
}

#[tokio::test]
async fn receipts_inventory_replay_and_repeated_restore_keep_deadline_and_verdict() {
    let store = Arc::new(MemoryReceipts::default());
    let a = durable(&store).await;
    register(&a, INSTANCE_A, empty_inventory()).await;
    let (job, request) = start_and_take_over(&a, INSTANCE_A).await;
    a.update_job(update(
        INSTANCE_A,
        &job.job_id,
        2,
        "completed",
        Some("done\n"),
        true,
    ))
    .await
    .unwrap();
    let deadline = {
        let mut rows = store.rows.lock().unwrap();
        rows[0].terminal_observed_at = now_ts() - JOB_TERMINAL_RETENTION_SECS + 10;
        rows[0].expires_at = rows[0].terminal_observed_at + JOB_TERMINAL_RETENTION_SECS;
        rows[0].expires_at
    };
    drop(a);
    for _ in 0..3 {
        let b = durable(&store).await;
        let snapshot =
            snapshot_from_request(&job, &request, "failed", 99, stream("changed\n", 1, false));
        register(
            &b,
            INSTANCE_A,
            ShellJobInventory {
                active_complete: true,
                jobs: vec![snapshot],
            },
        )
        .await;
        assert_eq!(b.get_job(&job.job_id).await.unwrap().status, "completed");
        assert_eq!(b.list_jobs(None).await.len(), 1);
        assert_eq!(store.rows.lock().unwrap()[0].expires_at, deadline);
        b.inner
            .lock()
            .await
            .jobs_by_id
            .get_mut(&job.job_id)
            .unwrap()
            .observation
            .receipt_expires_at = Some(now_ts());
        assert!(b.get_job(&job.job_id).await.is_err());
        assert!(b.list_jobs(None).await.is_empty());
        assert!(b.job_log(&job.job_id, None, None, None).await.is_err());
    }
}

#[tokio::test]
async fn receipts_failure_keeps_completed_result_and_reconciliation_lost_is_captured() {
    let store = Arc::new(MemoryReceipts {
        fail: true,
        ..Default::default()
    });
    let a = durable(&store).await;
    register(&a, INSTANCE_A, empty_inventory()).await;
    let (job, _) = start_and_take_over(&a, INSTANCE_A).await;
    a.update_job(update(
        INSTANCE_A,
        &job.job_id,
        2,
        "completed",
        Some("done\n"),
        true,
    ))
    .await
    .unwrap();
    assert_eq!(a.get_job(&job.job_id).await.unwrap().exit_code, Some(0));
    assert_eq!(
        a.job_log(&job.job_id, None, None, None)
            .await
            .unwrap()
            .1
            .as_deref(),
        Some("done\n")
    );
    assert!(store.rows.lock().unwrap().is_empty());
    let store = Arc::new(MemoryReceipts::default());
    let a = durable(&store).await;
    register(&a, INSTANCE_A, empty_inventory()).await;
    let (job, _) = start_and_take_over(&a, INSTANCE_A).await;
    register(&a, INSTANCE_B, empty_inventory()).await;
    assert_eq!(store.rows.lock().unwrap()[0].snapshot.status, "lost");
    drop(a);
    assert_eq!(
        durable(&store)
            .await
            .get_job(&job.job_id)
            .await
            .unwrap()
            .status,
        "lost"
    );
}

#[tokio::test]
async fn receipts_polling_completion_preserves_full_server_bounded_streams() {
    let store = Arc::new(MemoryReceipts::default());
    let a = durable(&store).await;
    let mut registration = register_request(INSTANCE_A, empty_inventory());
    registration.capabilities.job_state_reconciliation = false;
    registration.job_inventory = None;
    a.register(registration).await.unwrap();
    let (job, request) = start_and_take_over(&a, INSTANCE_A).await;
    a.complete(crate::runner_protocol::RunnerResultRequest {
        client_id: CLIENT_ID.into(),
        runner_instance_id: INSTANCE_A.into(),
        request_id: request.request_id,
        exit_code: Some(0),
        stdout: Some("stdout\n".repeat(100_000)),
        stderr: Some("stderr\n".repeat(100_000)),
        stdout_truncated: false,
        stderr_truncated: false,
        duration_ms: Some(20),
        error: None,
    })
    .await
    .unwrap();
    let original = a
        .job_log(&job.job_id, None, None, Some(100_000))
        .await
        .unwrap();
    assert!(original.0.stdout_log_truncated);
    assert!(
        store.rows.lock().unwrap()[0].snapshot.stdout.tail.len() > JOB_SNAPSHOT_STREAM_MAX_BYTES
    );
    drop(a);
    let b = durable(&store).await;
    let restored = b
        .job_log(&job.job_id, None, None, Some(100_000))
        .await
        .unwrap();
    assert_eq!(
        (&restored.1, &restored.2, restored.3, restored.4),
        (&original.1, &original.2, original.3, original.4)
    );
    assert_eq!(restored.0.status, "completed");
    assert_eq!(restored.0.exit_code, Some(0));
}

#[tokio::test]
async fn receipts_protocol_violation_and_recovery_sweep_capture_final_evidence() {
    let store = Arc::new(MemoryReceipts::default());
    let a = durable(&store).await;
    register(&a, INSTANCE_A, empty_inventory()).await;
    let (job, _) = start_and_take_over(&a, INSTANCE_A).await;
    let mut invalid = update(INSTANCE_A, &job.job_id, 1, "running", None, false);
    invalid.validation_progress = Some(ShellJobValidationProgress {
        completed: 0,
        current_step: None,
        failed_step: None,
    });
    a.update_job(invalid).await.unwrap();
    assert_eq!(store.rows.lock().unwrap()[0].snapshot.status, "failed");
    let (lost, _) = start_and_take_over(&a, INSTANCE_A).await;
    a.update_job(update(INSTANCE_A, &lost.job_id, 1, "running", None, false))
        .await
        .unwrap();
    drive_into_recovering(&a, &lost.job_id, INSTANCE_A).await;
    age_recovering_since(&a, &lost.job_id, job_recovery_grace_secs() + 1).await;
    recovery_timeout_sweep(&a).await;
    let rows = store.rows.lock().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter()
            .find(|row| row.snapshot.job_id == lost.job_id)
            .unwrap()
            .snapshot
            .status,
        "lost"
    );
}

#[tokio::test]
async fn receipts_validation_provenance_is_restored_only_by_same_instance_inventory() {
    let store = Arc::new(MemoryReceipts::default());
    let a = durable(&store).await;
    register(&a, INSTANCE_A, empty_inventory()).await;
    let (job, _) = start_and_take_over(&a, INSTANCE_A).await;
    {
        let mut inner = a.inner.lock().await;
        let record = inner.jobs_by_id.get_mut(&job.job_id).unwrap();
        record.validation = cargo_validation_start_metadata(None, None, None).validation;
        record.validation_steps = vec!["test".into()];
    }
    let mut terminal = update(INSTANCE_A, &job.job_id, 2, "completed", None, true);
    terminal.validation_progress = Some(ShellJobValidationProgress {
        completed: 1,
        current_step: None,
        failed_step: None,
    });
    let test_count_evidence = ShellJobTestCountEvidence {
        tests_detected: true,
        tests_run_count: Some(6),
        status: CargoTestCountEvidenceStatus::CompleteSummary,
    };
    terminal.test_count_evidence = Some(test_count_evidence.clone());
    a.update_job(terminal).await.unwrap();
    let (receipt_snapshot, mut injected_receipt) = {
        let rows = store.rows.lock().unwrap();
        let stored = &rows[0].snapshot;
        assert!(stored.context.validation.is_none());
        assert!(
            stored.test_count_evidence.is_none(),
            "receipt must not detach test-count evidence from its omitted validation identity"
        );
        (stored.clone(), rows[0].clone())
    };
    injected_receipt.snapshot.test_count_evidence = Some(test_count_evidence.clone());
    let injected_store = Arc::new(MemoryReceipts::default());
    injected_store.rows.lock().unwrap().push(injected_receipt);
    let rejected = durable(&injected_store).await;
    assert!(
        rejected.get_job(&job.job_id).await.is_err(),
        "receipt hydration must reject provenance-free test-count evidence"
    );
    drop(a);

    let mut snapshot = receipt_snapshot;
    let validation = cargo_validation_start_metadata(None, None, None).validation;
    snapshot.context.validation = validation.clone();
    snapshot.test_count_evidence = Some(test_count_evidence.clone());
    let inventory = ShellJobInventory {
        active_complete: true,
        jobs: vec![snapshot],
    };

    let different_instance = durable(&store).await;
    register(&different_instance, INSTANCE_B, inventory.clone()).await;
    let still_historical = different_instance.get_job(&job.job_id).await.unwrap();
    assert!(still_historical.validation.is_none());
    assert!(still_historical.test_count_evidence.is_none());
    drop(different_instance);

    let mut drifted_inventory = inventory.clone();
    drifted_inventory.jobs[0].duration_ms = Some(2_001);
    let drifted_same_instance = durable(&store).await;
    register(&drifted_same_instance, INSTANCE_A, drifted_inventory).await;
    let still_unproven = drifted_same_instance.get_job(&job.job_id).await.unwrap();
    assert!(still_unproven.validation.is_none());
    assert!(still_unproven.test_count_evidence.is_none());
    drop(drifted_same_instance);

    let same_instance = durable(&store).await;
    register(&same_instance, INSTANCE_A, inventory).await;
    let restored = same_instance.get_job(&job.job_id).await.unwrap();
    assert_eq!(restored.status, "completed");
    assert_eq!(restored.validation, validation);
    assert_eq!(restored.test_count_evidence, Some(test_count_evidence));
    assert!(restored.recovered_after_server_restart);
    assert!(same_instance.inner.lock().await.pending_by_id.is_empty());
}

#[tokio::test]
async fn receipts_unowned_admission_preserves_only_existing_global_visibility() {
    let store = Arc::new(MemoryReceipts::default());
    let a = durable(&store).await;
    let mut registration = register_request(INSTANCE_A, empty_inventory());
    registration.owner = None;
    a.register(registration).await.unwrap();
    let job = a
        .start_job(start_request("echo safe"), "bootstrap".into())
        .await
        .unwrap();
    a.stop_job(&job.job_id, "bootstrap".into()).await.unwrap();
    assert_eq!(store.rows.lock().unwrap().len(), 1);
    drop(a);
    let b = durable(&store).await;
    assert!(b
        .get_job_for_auth(Some(&access(Some("tester"), None)), &job.job_id)
        .await
        .is_err());
    let global = RunnerAccess {
        global_visibility: true,
        ..access(None, None)
    };
    assert!(b.get_job_for_auth(Some(&global), &job.job_id).await.is_ok());
    register(&b, INSTANCE_B, empty_inventory()).await;
    assert!(b
        .get_job_for_auth(Some(&access(Some("tester"), None)), &job.job_id)
        .await
        .is_err());
}

#[tokio::test]
async fn receipts_transient_storage_failure_retries_without_job_reexecution() {
    let store = Arc::new(MemoryReceipts {
        failures_remaining: std::sync::atomic::AtomicUsize::new(1),
        ..Default::default()
    });
    let registry = durable(&store).await;
    register(&registry, INSTANCE_A, empty_inventory()).await;
    let (job, _) = start_and_take_over(&registry, INSTANCE_A).await;
    registry
        .update_job(update(
            INSTANCE_A,
            &job.job_id,
            2,
            "completed",
            Some("done\n"),
            true,
        ))
        .await
        .unwrap();
    let observed = registry.get_job(&job.job_id).await.unwrap();
    assert_eq!(observed.exit_code, Some(0));
    let rows = store.rows.lock().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].snapshot.job_id, job.job_id);
}

#[tokio::test]
async fn passive_snapshot_keeps_recent_terminal_transition_beside_terminal_history() {
    let registry = RunnerRegistry::default();
    register(&registry, INSTANCE_A, empty_inventory()).await;

    let (old, _) = start_and_take_over(&registry, INSTANCE_A).await;
    registry
        .update_job(update(INSTANCE_A, &old.job_id, 1, "running", None, false))
        .await
        .unwrap();
    {
        let mut inner = registry.inner.lock().await;
        inner.jobs_by_id.get_mut(&old.job_id).unwrap().created_at = 1;
    }

    let mut newer = Vec::new();
    for index in 0..32 {
        let (job, _) = start_and_take_over(&registry, INSTANCE_A).await;
        registry
            .update_job(update(INSTANCE_A, &job.job_id, 1, "running", None, false))
            .await
            .unwrap();
        registry
            .update_job(update(INSTANCE_A, &job.job_id, 2, "completed", None, true))
            .await
            .unwrap();
        {
            let mut inner = registry.inner.lock().await;
            let record = inner.jobs_by_id.get_mut(&job.job_id).unwrap();
            record.created_at = 10 + index;
            record.observation.terminal_observed_at = Some(100 + index);
        }
        newer.push(job.job_id);
    }

    let baseline = registry
        .snapshot_jobs_for_auth_filtered(None, RUNTIME_PROJECT_ID, SESSION_ID, 64, 32)
        .await;
    assert!(baseline.iter().any(|snapshot| snapshot.job.job_id == old.job_id));
    assert_eq!(
        baseline.len(),
        33,
        "active baseline and terminal history have independent bounded pages"
    );

    registry
        .update_job(update(INSTANCE_A, &old.job_id, 2, "completed", None, true))
        .await
        .unwrap();
    {
        let mut inner = registry.inner.lock().await;
        inner
            .jobs_by_id
            .get_mut(&old.job_id)
            .unwrap()
            .observation
            .terminal_observed_at = Some(1_000);
    }
    let terminal = registry
        .snapshot_jobs_for_auth_filtered(None, RUNTIME_PROJECT_ID, SESSION_ID, 64, 32)
        .await;
    assert!(
        terminal.iter().any(|snapshot| snapshot.job.job_id == old.job_id),
        "a newly terminalized old Job must remain observable even with 32 newer terminal records"
    );
    assert_eq!(terminal.len(), 32);
    assert!(newer.iter().any(|id| terminal.iter().any(|snapshot| &snapshot.job.job_id == id)));
}

#[tokio::test]
async fn job_telemetry_contention_omits_snapshot_without_waiting_or_mutating() {
    let registry = RunnerRegistry::default();
    let guard = registry.inner.lock().await;
    assert!(registry.try_job_telemetry_snapshots_for_auth(None, &["unknown"]).is_none());
    assert!(guard.jobs_by_id.is_empty());
    drop(guard);
    assert!(registry.try_job_telemetry_snapshots_for_auth(None, &["unknown"]).unwrap().is_empty());
}

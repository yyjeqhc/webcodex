use super::*;
use crate::requests::{remove_pending_request_locked, retire_job_request_locked};
use crate::state::RunnerRegistryInner;

async fn enqueue(
    registry: &RunnerRegistry,
    client_id: &str,
) -> (String, tokio::sync::oneshot::Receiver<ShellRunResponse>) {
    registry
        .enqueue_run(
            ShellRunRequest {
                login: false,
                client_id: client_id.into(),
                cwd: None,
                command: "echo cleanup-fixture".into(),
                stdin: None,
                timeout_secs: 5,
                wait_timeout_secs: 0,
            },
            "tester".into(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn pending_cleanup_preserves_dispatch_certainty_and_other_queue_order() {
    for dispatched in [false, true] {
        let registry = RunnerRegistry::default();
        register_with_instance(&registry, "a", "inst-a").await;
        register_with_instance(&registry, "b", "inst-b").await;
        let (target, mut target_rx) = enqueue(&registry, "a").await;
        let (sibling, _sibling_rx) = enqueue(&registry, "a").await;
        let (foreign, _foreign_rx) = enqueue(&registry, "b").await;
        if dispatched {
            let request = registry
                .poll(RunnerPollRequest {
                    client_id: "a".into(),
                    runner_instance_id: "inst-a".into(),
                })
                .await
                .unwrap()
                .unwrap();
            assert_eq!(request.request_id, target);
        }
        assert_eq!(
            registry.cancel_request_dispatch_state(&target).await,
            Some(dispatched)
        );
        assert_eq!(registry.cancel_request_dispatch_state(&target).await, None);
        assert!(matches!(
            target_rx.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Closed)
        ));
        let inner = registry.inner.lock().await;
        assert!(!inner.pending_by_id.contains_key(&target));
        assert!(inner.pending_by_id.contains_key(&sibling));
        assert!(inner.pending_by_id.contains_key(&foreign));
        assert_eq!(
            inner.queues_by_runner["a"].iter().collect::<Vec<_>>(),
            vec![&sibling]
        );
        assert_eq!(
            inner.queues_by_runner["b"].iter().collect::<Vec<_>>(),
            vec![&foreign]
        );
    }
}

#[test]
fn missing_pending_cleanup_prunes_orphans_without_retiring_job_mapping() {
    let mut inner = RunnerRegistryInner::default();
    for client in ["a", "b"] {
        inner.queues_by_runner.insert(
            client.into(),
            ["before".into(), "orphan".into(), "after".into()].into(),
        );
    }
    inner
        .request_to_job
        .insert("orphan".into(), "historical-job".into());
    assert!(remove_pending_request_locked(&mut inner, "orphan").is_none());
    assert_eq!(inner.request_to_job["orphan"], "historical-job");
    for queue in inner.queues_by_runner.values() {
        assert_eq!(
            queue.iter().map(String::as_str).collect::<Vec<_>>(),
            ["before", "after"]
        );
    }
    assert!(remove_pending_request_locked(&mut inner, "orphan").is_none());
}

#[tokio::test]
async fn job_request_retirement_preserves_job_truth_and_separate_waiter_ownership() {
    for pending_present in [false, true] {
        let registry = RunnerRegistry::default();
        register_with_instance(&registry, "a", "inst-a").await;
        register_with_instance(&registry, "b", "inst-b").await;
        let job = registry
            .start_job(
                ShellJobOpRequest {
                    login: false,
                    op: "start".into(),
                    client_id: Some("a".into()),
                    cwd: None,
                    command: Some("echo retained-job".into()),
                    timeout_secs: Some(60),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                    codex: None,
                },
                "alice".into(),
            )
            .await
            .unwrap();
        let (sibling, _sibling_rx) = enqueue(&registry, "a").await;
        let (foreign, _foreign_rx) = enqueue(&registry, "b").await;
        let mut inner = registry.inner.lock().await;
        let request_id = inner.jobs_by_id[&job.job_id].request_id.clone().unwrap();
        assert_eq!(inner.request_to_job[&request_id], job.job_id);
        let before =
            serde_json::to_value(crate::jobs::job_view(&inner.jobs_by_id[&job.job_id])).unwrap();
        if !pending_present {
            // A prior consumer may have taken the pending record while a
            // terminal/recovery caller still owns the exact Job request tuple.
            inner.pending_by_id.remove(&request_id);
        }
        let (waiter, mut waiter_rx) = tokio::sync::oneshot::channel();
        inner.persistent_waiters.insert(request_id.clone(), waiter);
        for _ in 0..2 {
            retire_job_request_locked(&mut inner, "a", &request_id);
            assert!(!inner.pending_by_id.contains_key(&request_id));
            assert!(!inner.request_to_job.contains_key(&request_id));
            assert_eq!(
                inner.queues_by_runner["a"].iter().collect::<Vec<_>>(),
                vec![&sibling]
            );
            assert_eq!(
                inner.queues_by_runner["b"].iter().collect::<Vec<_>>(),
                vec![&foreign]
            );
            assert!(inner.pending_by_id.contains_key(&sibling));
            assert!(inner.pending_by_id.contains_key(&foreign));
            assert_eq!(
                serde_json::to_value(crate::jobs::job_view(&inner.jobs_by_id[&job.job_id]))
                    .unwrap(),
                before
            );
            assert!(matches!(
                waiter_rx.try_recv(),
                Err(tokio::sync::oneshot::error::TryRecvError::Empty)
            ));
        }
    }
}

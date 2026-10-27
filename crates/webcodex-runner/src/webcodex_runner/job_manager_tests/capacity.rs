use super::*;

#[test]
fn hot_capacity_growth_wakes_fifo_and_shrink_drains_without_interrupting_jobs() {
    let temp = tempfile::tempdir().unwrap();
    let helper = structured_process_helper();
    let (sink, _rx) = structured_test_sink("structured-agent", "structured-instance");
    let manager = JobManager::new(2);
    let clone = manager.clone();
    let jobs = (0..8)
        .map(|i| GatedStructuredJob::new(temp.path(), &format!("resize-{i}")))
        .collect::<Vec<_>>();
    for job in &jobs {
        enqueue_gated_structured_job(&manager, &sink, temp.path(), &helper, job);
    }
    wait_for_all_started(&jobs[..2]);
    assert!(jobs[2..].iter().all(|job| !job.started.exists()));
    let before = manager.inventory();
    clone.commit_concurrency_limit(6);
    clone.start_available_queued();
    assert_eq!(manager.concurrency_limit(), 6);
    // No original child has been released: growth itself must start the queue.
    wait_for_all_started(&jobs[..6]);
    assert!(!jobs[6].started.exists());
    clone.commit_concurrency_limit(2);
    clone.start_available_queued();
    assert_eq!(manager.concurrency_limit(), 2);
    assert_eq!(
        lock_unpoison(&manager.jobs)
            .values()
            .filter(|job| job.slot_reserved)
            .count(),
        6
    );
    for job in &jobs[..4] {
        job.release();
    }
    assert!(wait_until(Duration::from_secs(10), || manager
        .inventory()
        .jobs
        .iter()
        .filter(|j| j.status == "completed")
        .count()
        == 4));
    assert!(!jobs[6].started.exists());
    jobs[4].release();
    wait_for_all_started(&jobs[6..7]);
    assert!(!jobs[7].started.exists());
    let after = manager.inventory();
    for original in before.jobs {
        let current = after
            .jobs
            .iter()
            .find(|j| j.job_id == original.job_id)
            .unwrap();
        assert_eq!(current.request_id, original.request_id);
        assert_eq!(current.context, original.context);
    }
    for job in &jobs[5..] {
        job.release();
    }
    wait_for_job_workers(&manager);
    for job in &jobs {
        assert_gated_job_started_once(job);
    }
}

#[test]
fn capacity_changes_race_enqueue_completion_stop_and_shutdown_without_replay() {
    let temp = tempfile::tempdir().unwrap();
    let helper = structured_process_helper();
    let (sink, _rx) = structured_test_sink("structured-agent", "structured-instance");
    let manager = JobManager::new(2);
    let jobs = (0..12)
        .map(|i| GatedStructuredJob::new(temp.path(), &format!("race-{i}")))
        .collect::<Vec<_>>();
    for job in &jobs[..6] {
        enqueue_gated_structured_job(&manager, &sink, temp.path(), &helper, job);
    }
    wait_for_all_started(&jobs[..2]);
    let barrier = std::sync::Barrier::new(4);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            barrier.wait();
            for limit in [6, 2, 6, 2] {
                manager.commit_concurrency_limit(limit);
                manager.start_available_queued();
            }
        });
        scope.spawn(|| {
            barrier.wait();
            for job in &jobs[6..] {
                enqueue_gated_structured_job(&manager, &sink, temp.path(), &helper, job);
            }
        });
        scope.spawn(|| {
            barrier.wait();
            let _ = manager.stop(&jobs[5].job_id);
            jobs[0].release();
            jobs[1].release();
        });
        barrier.wait();
    });
    std::thread::scope(|scope| {
        scope.spawn(|| {
            manager.commit_concurrency_limit(6);
            manager.start_available_queued();
        });
        scope.spawn(|| {
            manager.stop_accepting_work();
            manager.cancel_queued_for_shutdown();
        });
    });
    for job in &jobs {
        job.release();
    }
    wait_for_job_workers(&manager);
    assert!(lock_unpoison(&manager.queued).is_empty());
    assert!(manager
        .inventory()
        .jobs
        .iter()
        .all(|j| !runner_job_is_active(&j.status)));
    for job in &jobs {
        if job.started.exists() {
            assert_gated_job_started_once(job);
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn hot_capacity_preserves_interactive_detached_and_structured_job_identity() {
    let temp = tempfile::tempdir().unwrap();
    let helper = structured_process_helper();
    let (sink, _rx) = structured_test_sink("structured-agent", "structured-instance");
    let manager = JobManager::new(1);
    *lock_unpoison(&manager.detached_store_root_override) =
        Some(temp.path().join("detached-state"));
    let jobs = (0..5)
        .map(|i| GatedStructuredJob::new(temp.path(), &format!("mixed-{i}")))
        .collect::<Vec<_>>();
    for (i, job) in jobs.iter().enumerate() {
        match i {
            0 | 3 => {
                let mut context = structured_process_context(temp.path(), job.args().len(), false);
                context
                    .structured_execution
                    .as_mut()
                    .unwrap()
                    .execution_source = "run_process_interactive".into();
                manager.enqueue(sink.clone(), PendingJobStart::from_wire(
                    1, RunnerPolicy { allow_cwd_anywhere: true, ..RunnerPolicy::default() },
                    ShellConfig::default(), SshConfig::default(), temp.path().join("project-registry"),
                    serde_json::from_value(json!({
                        "request_id": format!("request-{}", job.job_id), "client_id":"structured-agent",
                        "kind":"start_interactive_process_job", "job_id":job.job_id, "cwd":temp.path(),
                        "command":"", "process":{"executable":helper.path,"args":job.args()},
                        "timeout_secs":20,"requested_by":"test","created_at":chrono::Utc::now().timestamp(),
                        "job_context":context,
                    })).unwrap(),
                ));
            }
            1 => enqueue_detached_process_job(
                &manager,
                sink.clone(),
                temp.path(),
                &job.job_id,
                &helper.path,
                job.args(),
                None,
                20,
            ),
            _ => enqueue_gated_structured_job(&manager, &sink, temp.path(), &helper, job),
        }
    }
    wait_for_all_started(&jobs[..1]);
    let before = manager.inventory();
    manager.commit_concurrency_limit(4);
    manager.start_available_queued();
    wait_for_all_started(&jobs[..4]);
    assert!(wait_until(Duration::from_secs(5), || manager
        .inventory()
        .jobs
        .iter()
        .filter(|job| job.status == "running")
        .count()
        == 4));
    manager.clone().commit_concurrency_limit(1);
    manager.start_available_queued();
    assert_eq!(active_gated_children(&jobs), 4);
    assert!(!jobs[4].started.exists());
    for job in &jobs[..3] {
        job.release();
    }
    assert!(wait_until(Duration::from_secs(10), || manager
        .inventory()
        .jobs
        .iter()
        .filter(|job| job.status == "completed")
        .count()
        == 3));
    assert!(!jobs[4].started.exists());
    jobs[3].release();
    wait_for_all_started(&jobs[4..]);
    jobs[4].release();
    wait_for_job_workers(&manager);
    let after = manager.inventory();
    for original in before.jobs {
        let current = after
            .jobs
            .iter()
            .find(|job| job.job_id == original.job_id)
            .unwrap();
        assert_eq!(current.status, "completed");
        assert_eq!(current.request_id, original.request_id);
        assert_eq!(current.context, original.context);
    }
    for job in &jobs {
        assert_gated_job_started_once(job);
    }
}

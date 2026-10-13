use super::*;
use crate::state::{JobLifecycleState, ShellJobVisibility};
use std::collections::HashSet;

async fn fixture() -> (RunnerRegistry, String) {
    let registry = RunnerRegistry::default();
    register_with_instance(&registry, "oe", "index-instance").await;
    let job = registry
        .start_job_with_metadata(
            ShellJobOpRequest {
                login: false,
                op: "start".into(),
                client_id: Some("oe".into()),
                cwd: None,
                command: Some("echo index-fixture".into()),
                timeout_secs: Some(60),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "alice".into(),
            ShellJobStartMetadata {
                project_id: Some("agent:oe:target".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    (registry, job.job_id)
}

#[tokio::test]
async fn console_aggregates_omit_project_bodies_and_batched_authority_matches_exact_queries() {
    let (registry, _) = fixture().await;
    {
        let mut inner = registry.inner.lock().await;
        let runner = inner.runners.get_mut("oe").unwrap();
        runner.projects = (0..4096).map(|i| {
            let mut value = serde_json::json!({"id":format!("p{i}"),"path":"/private-no-projection","updated_at":1,"disabled":i==4095});
            if i % 4 != 0 {
                value["lineage"] = serde_json::json!({"kind":"managed_worktree_source","source_project_id":format!("p{}",i/4*4),"source_root_fingerprint":"root","base_sha":"a".repeat(40)});
            }
            serde_json::from_value(value).unwrap()
        }).collect();
    }
    let auth = auth_context(Some("alice"), false);
    let before = registry.full_job_history_scan_count_for_test();
    let summary = registry
        .console_registry_snapshot_for_auth(Some(&auth), true)
        .await;
    assert_eq!(summary.projects_by_runner["oe"], 4095);
    assert_eq!(summary.project_families, 1024);
    assert_eq!(summary.runners.len(), 1);
    assert!(summary.runners[0].projects.is_empty());
    assert!(!serde_json::to_string(&summary.runners)
        .unwrap()
        .contains("private-no-projection"));
    let ids = vec![
        "agent:oe:p0".into(),
        "agent:oe:p4095".into(),
        "agent:oe:missing".into(),
        "agent:foreign:p1".into(),
    ];
    for observer in [&auth, &auth_context(Some("bob"), false)] {
        let batch = registry
            .visible_project_ids_for_auth_snapshot(Some(observer), &ids)
            .await;
        for id in &ids {
            assert_eq!(
                batch.contains(id),
                registry
                    .exact_project_visible_for_auth_snapshot(Some(observer), id)
                    .await
            );
        }
    }
    assert_eq!(registry.full_job_history_scan_count_for_test(), before);
    let no_projects = registry
        .console_registry_snapshot_for_auth(Some(&auth), false)
        .await;
    assert!(no_projects.projects_by_runner.is_empty());
    assert_eq!(no_projects.project_families, 0);
    eprintln!("CONSOLE_AGGREGATE registered_projects=4096 enabled=4095 families=1024 project_bodies=0 full_job_scans=0");
}

#[tokio::test]
async fn optional_job_snapshot_skips_contention_and_expired_budget() {
    let (registry, _) = fixture().await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    let guard = registry.inner.lock().await;
    assert!(registry
        .try_snapshot_jobs_for_auth_filtered(None, "agent:oe:target", "session", 8, 8, deadline)
        .is_none());
    drop(guard);
    assert!(registry
        .try_snapshot_jobs_for_auth_filtered(
            None,
            "agent:oe:target",
            "session",
            8,
            8,
            std::time::Instant::now()
        )
        .is_none());
    assert!(registry
        .try_snapshot_jobs_for_auth_filtered(None, "agent:oe:target", "session", 8, 8, deadline)
        .is_some());
}

async fn assert_index(registry: &RunnerRegistry) {
    let mut inner = registry.inner.lock().await;
    let canonical: HashSet<_> = inner
        .jobs_by_id
        .iter()
        .filter(|(_, job)| job.lifecycle.is_active())
        .map(|(id, _)| id.clone())
        .collect();
    let indexed: HashSet<_> = inner.jobs_by_id.active_ids().into_iter().collect();
    assert_eq!(indexed, canonical);
}

#[tokio::test]
async fn active_index_tracks_every_mutation_including_same_lock_and_removal() {
    let (registry, id) = fixture().await;
    for status in [
        "queued",
        "agent_queued",
        "running",
        "stop_requested",
        "completed",
        "failed",
        "stopped",
        "cancelled",
        "timeout",
        "lost",
    ] {
        {
            let mut inner = registry.inner.lock().await;
            inner.jobs_by_id.get_mut(&id).unwrap().lifecycle =
                crate::jobs::parse_job_lifecycle(status).unwrap();
            let indexed = inner.jobs_by_id.active_ids();
            assert_eq!(
                indexed.contains(&id),
                inner.jobs_by_id[&id].lifecycle.is_active(),
                "{status}"
            );
        }
        assert_index(&registry).await;
    }
    let mut inner = registry.inner.lock().await;
    let mut restored = inner.jobs_by_id[&id].clone();
    restored.job_id = "restored-active".into();
    restored.lifecycle = JobLifecycleState::Running;
    inner.jobs_by_id.insert(restored.job_id.clone(), restored);
    assert!(inner
        .jobs_by_id
        .active_ids()
        .contains(&"restored-active".to_string()));
    inner.jobs_by_id.remove("restored-active");
    assert!(inner.jobs_by_id.active_ids().is_empty());
}

#[tokio::test]
async fn ten_thousand_terminal_jobs_do_not_enter_project_count_refresh() {
    let (registry, id) = fixture().await;
    {
        let mut inner = registry.inner.lock().await;
        let template = inner.jobs_by_id[&id].clone();
        for index in 0..10_000 {
            let mut job = template.clone();
            job.job_id = format!("historical-{index}");
            job.lifecycle = JobLifecycleState::Completed;
            inner.jobs_by_id.insert(job.job_id.clone(), job);
        }
        for (id, visibility, owner, project) in [
            (
                "private",
                ShellJobVisibility::HiddenUntilHandoff,
                "alice",
                "agent:oe:target",
            ),
            (
                "cleanup",
                ShellJobVisibility::CleanupPending,
                "alice",
                "agent:oe:target",
            ),
            (
                "foreign",
                ShellJobVisibility::Public,
                "bob",
                "agent:oe:target",
            ),
            (
                "other-project",
                ShellJobVisibility::Public,
                "alice",
                "agent:oe:other",
            ),
        ] {
            let mut job = template.clone();
            job.job_id = id.into();
            job.visibility = visibility;
            job.project_id = Some(project.into());
            if owner == "bob" {
                job.auth_group = shared_key_access("foreign").group;
            }
            inner.jobs_by_id.insert(job.job_id.clone(), job);
        }
    }
    assert_index(&registry).await;
    let alice = auth_context(Some("alice"), false);
    let before = registry.project_job_candidate_refresh_count_for_test();
    let counts = registry
        .count_active_jobs_for_projects(Some(&alice), &["agent:oe:target"])
        .await;
    assert_eq!(counts["agent:oe:target"], 1);
    let refreshed = registry.project_job_candidate_refresh_count_for_test() - before;
    assert_eq!(refreshed, 1);
    assert_eq!(
        registry
            .active_job_counts_by_runner_for_auth(Some(&alice))
            .await["oe"],
        2
    );
    // Unregister still includes hidden/cleanup work, unlike user-visible counts.
    assert_eq!(
        registry
            .begin_project_unregister(Some(&alice), "agent:oe:target")
            .await
            .unwrap(),
        3
    );
    eprintln!("ACTIVE_JOB_INDEX terminal=10000 active=5 requested_projects=1 refreshed={refreshed} visible_count=1");
}

fn expected_runtime_job_aggregate(
    jobs: &[webcodex_core::runner_protocol::ShellJobInfo],
) -> crate::RuntimeJobAggregateSnapshot {
    use webcodex_core::runner_job_lifecycle::RunnerJobLifecycle;
    let mut result = crate::RuntimeJobAggregateSnapshot::default();
    for job in jobs {
        let status = job.status.as_str();
        let lifecycle = RunnerJobLifecycle::from_wire(status);
        let active = crate::job_status::job_status_is_active(status);
        let running = matches!(
            lifecycle,
            Ok(RunnerJobLifecycle::Running | RunnerJobLifecycle::StartedLegacy)
        );
        let queued = matches!(
            lifecycle,
            Ok(RunnerJobLifecycle::Queued | RunnerJobLifecycle::RunnerQueued)
        );
        let slot = lifecycle.is_ok_and(RunnerJobLifecycle::occupies_execution_slot);
        result.active_count += usize::from(active);
        result.running_count += usize::from(running);
        result.queued_count += usize::from(queued);
        result.recovering_count += usize::from(status == "recovering");
        result.lost_after_reconcile_count += usize::from(
            status == "lost"
                && matches!(
                    job.recovery_reason_code.as_deref(),
                    Some(
                        "runner_inventory_missing"
                            | "runner_instance_replaced"
                            | "runner_recovery_deadline_exceeded"
                    )
                ),
        );
        if active || slot || queued {
            let row = result.by_runner.entry(job.client_id.clone()).or_default();
            row.active += usize::from(active);
            row.running += usize::from(slot);
            row.queued += usize::from(queued);
        }
    }
    result
}

async fn populated_runtime_job_aggregate_fixture(count: usize) -> RunnerRegistry {
    let (registry, id) = fixture().await;
    {
        let mut inner = registry.inner.lock().await;
        let template = inner.jobs_by_id[&id].clone();
        for index in 0..count {
            let mut job = template.clone();
            job.job_id = format!("historical-runtime-{index}");
            job.lifecycle = match index % 6 {
                0 => JobLifecycleState::Completed,
                1 => JobLifecycleState::Failed,
                2 => JobLifecycleState::Stopped,
                3 => JobLifecycleState::Lost,
                4 => JobLifecycleState::Cancelled,
                _ => JobLifecycleState::TimedOut,
            };
            job.command_preview = "read-only synthetic job metadata".repeat(45);
            job.validation_steps = vec!["validation evidence".repeat(16); 10];
            inner.jobs_by_id.insert(job.job_id.clone(), job);
        }
    }
    registry
}

#[tokio::test]
async fn runtime_status_aggregate_matches_full_visible_job_projection_without_log_copies() {
    let registry = populated_runtime_job_aggregate_fixture(240).await;
    for auth in [
        None,
        Some(auth_context(Some("alice"), false)),
        Some(auth_context(Some("bob"), false)),
    ] {
        let access = auth.as_ref();
        let old = registry.list_all_jobs_for_auth(access).await;
        for client_id in [None, Some("oe"), Some("missing")] {
            let selected = old
                .iter()
                .filter(|job| client_id.is_none_or(|id| job.client_id == id))
                .cloned()
                .collect::<Vec<_>>();
            let expected = expected_runtime_job_aggregate(&selected);
            let actual = registry
                .runtime_job_aggregate_for_auth(access, client_id)
                .await;
            assert_eq!(actual, expected, "client_id={client_id:?}");
        }
    }
}

#[tokio::test]
async fn runtime_status_aggregate_preserves_recovery_overlay_and_capacity_distinctions() {
    use crate::state::{JobRecoveryPhase, JobRecoveryReason, JobRecoveryState};

    let (registry, id) = fixture().await;
    for lifecycle in [
        JobLifecycleState::Queued,
        JobLifecycleState::RunnerQueued,
        JobLifecycleState::StartedLegacy,
        JobLifecycleState::Running,
        JobLifecycleState::StopRequested,
        JobLifecycleState::Completed,
        JobLifecycleState::Failed,
        JobLifecycleState::Stopped,
        JobLifecycleState::Timeout,
        JobLifecycleState::TimedOut,
        JobLifecycleState::Lost,
        JobLifecycleState::Cancelled,
    ] {
        for recovering in [false, true] {
            for reason in [
                None,
                Some(JobRecoveryReason::RunnerInventoryMissing),
                Some(JobRecoveryReason::RunnerInstanceReplaced),
                Some(JobRecoveryReason::RunnerRecoveryDeadlineExceeded),
                Some(JobRecoveryReason::RunnerTransportDisconnected),
            ] {
                {
                    let mut inner = registry.inner.lock().await;
                    let job = inner.jobs_by_id.get_mut(&id).unwrap();
                    job.lifecycle = lifecycle;
                    job.recovery = JobRecoveryState {
                        phase: recovering.then_some(JobRecoveryPhase::Recovering),
                        reason,
                        recovering_since: recovering.then_some(crate::now_ts()),
                        ..Default::default()
                    };
                }
                // Keep the established public projection as an independent
                // oracle, including terminal Jobs with stale recovery metadata.
                let visible = registry.list_all_jobs_for_auth(None).await;
                let actual = registry.runtime_job_aggregate_for_auth(None, None).await;
                assert_eq!(
                    actual,
                    expected_runtime_job_aggregate(&visible),
                    "{lifecycle:?} recovering={recovering} reason={reason:?}"
                );
                let capacity = registry.active_job_summary_by_runner_for_auth(None).await;
                if lifecycle.is_active() {
                    assert_eq!(capacity["oe"].active, 1);
                    assert_eq!(
                        capacity["oe"].running,
                        usize::from(lifecycle.occupies_execution_slot())
                    );
                } else {
                    assert!(capacity.is_empty());
                }
                if recovering && lifecycle.is_runner_active() {
                    assert_eq!(actual.recovering_count, 1);
                    assert_eq!(actual.running_count, 0);
                    assert_eq!(actual.queued_count, 0);
                    assert_eq!(actual.by_runner["oe"].running, 0);
                }
            }
        }
    }
}

#[tokio::test]
#[ignore = "manual 1,500-job typed status aggregate versus full Job DTO cost comparison"]
async fn runtime_status_aggregate_cost_comparison() {
    use std::time::Instant;
    let registry = populated_runtime_job_aggregate_fixture(1_500).await;
    let auth = auth_context(Some("alice"), false);
    let reference = registry.list_all_jobs_for_auth(Some(&auth)).await;
    let expected = expected_runtime_job_aggregate(&reference);
    assert_eq!(
        registry
            .runtime_job_aggregate_for_auth(Some(&auth), None)
            .await,
        expected
    );
    let mut full_ns = 0_u128;
    let mut aggregate_ns = 0_u128;
    let rounds = 16_u128;
    for index in 0..rounds {
        if index % 2 == 0 {
            let t = Instant::now();
            std::hint::black_box(registry.list_all_jobs_for_auth(Some(&auth)).await);
            full_ns += t.elapsed().as_nanos();
        }
        let t = Instant::now();
        std::hint::black_box(
            registry
                .runtime_job_aggregate_for_auth(Some(&auth), None)
                .await,
        );
        aggregate_ns += t.elapsed().as_nanos();
        if index % 2 != 0 {
            let t = Instant::now();
            std::hint::black_box(registry.list_all_jobs_for_auth(Some(&auth)).await);
            full_ns += t.elapsed().as_nanos();
        }
    }
    println!("RUNTIME_STATUS_JOB_AGGREGATE retained_jobs=1500 full_ms={:.3} aggregate_ms={:.3} speedup={:.2}x",
        full_ns as f64 / rounds as f64 / 1e6,
        aggregate_ns as f64 / rounds as f64 / 1e6,
        full_ns as f64 / aggregate_ns as f64,
    );
}

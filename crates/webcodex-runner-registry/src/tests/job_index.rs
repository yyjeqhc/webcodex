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

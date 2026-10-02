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

use super::*;
use webcodex_runner_registry::{JobRecoveryPhase, JobRecoveryReason};

fn key(window: &str) -> AttentionKey {
    AttentionKey {
        principal_kind: "user".into(),
        principal_id: "owner".into(),
        window_key: window.into(),
        project: "project".into(),
        session_id: "business-session".into(),
    }
}

fn job(id: usize, status: &str) -> JobAttentionSnapshot {
    JobAttentionSnapshot {
        job: serde_json::from_value(json!({"job_id": format!("job-{id}"), "client_id": "runner", "command_preview": "PRIVATE", "status": status, "created_at": 1})).unwrap(),
        validation_output: None, recovery: None,
    }
}

fn project(cursor: &JobAttentionCursor, jobs: &[JobAttentionSnapshot]) -> ToolResult {
    let mut result = ToolResult::ok(json!({"main": "preserved"}));
    cursor.project_result(
        &mut result,
        key("window"),
        jobs,
        None,
        |snapshot| json!({"job_id": snapshot.job.job_id}),
    );
    result
}

#[test]
fn job_attention_ignores_routine_lifecycle_timestamps_and_progress() {
    let cursor = JobAttentionCursor::default();
    let mut snapshot = job(1, "queued");
    for (index, status) in [
        "queued",
        "agent_queued",
        "started",
        "running",
        "running",
        "stop_requested",
    ]
    .into_iter()
    .enumerate()
    {
        snapshot.job.status = status.into();
        snapshot.job.started_at = Some(index as i64);
        snapshot.job.last_update_seq = Some(index as u64);
        snapshot.job.reconciled_at = Some(index as i64);
        assert!(
            project(&cursor, &[snapshot.clone()])
                .output
                .get("job_attention")
                .is_none(),
            "{status}"
        );
    }
    for status in [
        "completed",
        "failed",
        "stopped",
        "cancelled",
        "timeout",
        "timed_out",
        "lost",
    ] {
        // Each distinct accepted terminal revision is meaningful; repeated reads aren't.
        snapshot.job.status = status.into();
        assert_eq!(
            project(&cursor, &[snapshot.clone()]).output["job_attention"]["items"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        snapshot.job.ended_at = Some(999);
        assert!(project(&cursor, &[snapshot.clone()])
            .output
            .get("job_attention")
            .is_none());
    }
}

#[test]
fn job_attention_recovery_uses_typed_overlay_not_status_text_or_timestamps() {
    let cursor = JobAttentionCursor::default();
    let mut snapshot = job(1, "running");
    assert!(project(&cursor, &[snapshot.clone()])
        .output
        .get("job_attention")
        .is_none());
    snapshot.recovery = Some((
        JobRecoveryPhase::Recovering,
        Some(JobRecoveryReason::RunnerTransportDisconnected),
    ));
    assert!(project(&cursor, &[snapshot.clone()])
        .output
        .get("job_attention")
        .is_some());
    snapshot.job.reconciled_at = Some(20);
    assert!(project(&cursor, &[snapshot.clone()])
        .output
        .get("job_attention")
        .is_none());
    snapshot.recovery = Some((
        JobRecoveryPhase::Reconciled,
        Some(JobRecoveryReason::SameInstanceReconciliation),
    ));
    assert!(project(&cursor, &[snapshot.clone()])
        .output
        .get("job_attention")
        .is_some());
    assert!(project(&cursor, &[snapshot.clone()])
        .output
        .get("job_attention")
        .is_none());
    let restarted = JobAttentionCursor::default();
    assert!(project(&restarted, &[snapshot.clone()])
        .output
        .get("job_attention")
        .is_some());
    assert!(project(&restarted, &[snapshot])
        .output
        .get("job_attention")
        .is_none());
}

#[test]
fn job_attention_bounded_delivery_does_not_consume_undelivered_terminals() {
    let cursor = JobAttentionCursor::default();
    let mut jobs: Vec<_> = (0..MAX_ACTIVE_JOBS_PER_KEY)
        .map(|id| job(id, "running"))
        .collect();
    assert!(project(&cursor, &jobs)
        .output
        .get("job_attention")
        .is_none());
    for snapshot in &mut jobs {
        snapshot.job.status = "failed".into();
    }
    let mut delivered = HashSet::new();
    for _ in 0..MAX_ACTIVE_JOBS_PER_KEY / MAX_ITEMS {
        let result = project(&cursor, &jobs);
        let items = result.output["job_attention"]["items"].as_array().unwrap();
        assert_eq!(items.len(), MAX_ITEMS);
        for item in items {
            assert!(delivered.insert(item["job_id"].as_str().unwrap().to_string()));
        }
    }
    assert_eq!(delivered.len(), MAX_ACTIVE_JOBS_PER_KEY);
    assert!(project(&cursor, &jobs)
        .output
        .get("job_attention")
        .is_none());
}

#[test]
fn job_attention_omission_preserves_main_result_and_pending_delivery() {
    let cursor = JobAttentionCursor::default();
    let mut snapshot = job(1, "running");
    project(&cursor, &[snapshot.clone()]);
    snapshot.job.status = "completed".into();
    let mut result = ToolResult::ok(
        json!({"large": "x".repeat(webcodex_workspace::file_read_range::MAX_SERIALIZED_OUTPUT_BYTES)}),
    );
    let before = serde_json::to_value(&result).unwrap();
    cursor.project_result(
        &mut result,
        key("window"),
        &[snapshot.clone()],
        None,
        |snapshot| json!({"job_id": snapshot.job.job_id}),
    );
    assert_eq!(serde_json::to_value(&result).unwrap(), before);
    assert!(project(&cursor, &[snapshot])
        .output
        .get("job_attention")
        .is_some());
}

#[test]
fn job_attention_pending_handoff_does_not_consume_racing_terminal_truth() {
    let cursor = JobAttentionCursor::default();
    let snapshot = job(1, "completed");
    let mut pending = ToolResult::ok(json!({"execution_state": "pending"}));
    cursor.project_result(
        &mut pending,
        key("window"),
        &[snapshot.clone()],
        Some("job-1"),
        |snapshot| json!({"job_id": snapshot.job.job_id}),
    );
    assert!(pending.output.get("job_attention").is_none());
    assert!(project(&cursor, &[snapshot.clone()])
        .output
        .get("job_attention")
        .is_some());
    assert!(project(&cursor, &[snapshot])
        .output
        .get("job_attention")
        .is_none());
}

#[test]
fn job_attention_terminal_delivery_precedes_active_recovery_under_item_budget() {
    let cursor = JobAttentionCursor::default();
    let mut jobs: Vec<_> = (0..=MAX_ITEMS).map(|id| job(id, "running")).collect();
    project(&cursor, &jobs);
    for snapshot in &mut jobs[..MAX_ITEMS] {
        snapshot.recovery = Some((
            JobRecoveryPhase::Recovering,
            Some(JobRecoveryReason::RunnerTransportDisconnected),
        ));
    }
    jobs[MAX_ITEMS].job.status = "completed".into();
    let first = project(&cursor, &jobs);
    assert_eq!(
        first.output["job_attention"]["items"][0]["job_id"],
        format!("job-{}", MAX_ITEMS)
    );
    assert_eq!(
        first.output["job_attention"]["items"]
            .as_array()
            .unwrap()
            .len(),
        MAX_ITEMS
    );
    assert_eq!(
        project(&cursor, &jobs).output["job_attention"]["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

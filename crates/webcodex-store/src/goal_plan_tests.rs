use super::goal::{GoalLifecycle, GoalPatch, NewGoal};
use super::goal_plan::*;
use super::store_primitives::{CommunicationPrincipal, COMMUNICATION_PRINCIPAL_DIGEST_PREFIX};
use super::Database;
use rusqlite::params;

const T0: i64 = 10_000;

fn fixture() -> (tempfile::TempDir, Database, CommunicationPrincipal) {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("goal-plan.db")).unwrap();
    let owner = CommunicationPrincipal {
        kind: "user".into(),
        digest: format!("{COMMUNICATION_PRINCIPAL_DIGEST_PREFIX}{}", "a".repeat(64)),
    };
    (temp, db, owner)
}

fn input(key: &str) -> NewGoal {
    NewGoal {
        title: "Goal workflow".into(),
        objective: "Implement, validate and review".into(),
        controller_agent_id: None,
        completion_conditions: vec!["Focused validation and independent review pass".into()],
        steps: ["inspect", "implement", "verify"]
            .into_iter()
            .map(|id| NewGoalStep {
                id: id.into(),
                title: id.into(),
            })
            .collect(),
        idempotency_key: key.into(),
    }
}

fn checkpoint(done: &[&str], current: Option<&str>, summary: &str) -> GoalCheckpoint {
    GoalCheckpoint {
        completed_step_ids: done.iter().map(|id| (*id).into()).collect(),
        current_step_id: current.map(str::to_string),
        summary: summary.into(),
    }
}

#[test]
fn goal_plan_create_read_checkpoint_and_exact_replay_survive_reopen() {
    let (temp, db, owner) = fixture();
    let created = db.create_goal_at(&owner, input("create"), T0).unwrap();
    let id = created.goal.summary.goal_id;
    assert_eq!(created.goal.plan.steps.len(), 3);
    assert!(created
        .goal
        .plan
        .steps
        .iter()
        .all(|step| step.status == GoalStepStatus::Pending && step.updated_at_unix_ms == T0));
    let first = checkpoint(
        &["inspect"],
        Some("implement"),
        "Inspection complete; implement the plan transaction",
    );
    let result = db
        .checkpoint_goal_at(&owner, &id, 1, first.clone(), "checkpoint", T0 + 1)
        .unwrap();
    assert_eq!(result.goal.summary.revision, 2);
    assert_eq!(result.goal.plan.completed_count(), 1);
    assert_eq!(result.goal.plan.current_step().unwrap().id, "implement");
    assert_eq!(result.goal.plan.checkpoint_at_unix_ms, Some(T0 + 1));
    assert_eq!(result.goal.plan.steps[0].updated_at_unix_ms, T0 + 1);
    let replay = db
        .checkpoint_goal_at(&owner, &id, 1, first.clone(), "checkpoint", T0 + 2)
        .unwrap();
    assert!(replay.replayed);
    assert!(!replay.state_changed);
    assert_eq!(replay.goal, result.goal);
    drop(db);
    let reopened = Database::open(&temp.path().join("goal-plan.db")).unwrap();
    assert_eq!(reopened.read_goal(&owner, &id).unwrap(), result.goal);
    let replay = reopened
        .checkpoint_goal_at(&owner, &id, 1, first.clone(), "checkpoint", T0 + 3)
        .unwrap();
    assert_eq!(replay.goal.summary.revision, 2);
    let mut changed = first;
    changed.summary.push_str(" changed");
    assert_eq!(
        reopened
            .checkpoint_goal_at(&owner, &id, 1, changed, "checkpoint", T0 + 4)
            .unwrap_err()
            .code(),
        "goal_idempotency_conflict"
    );
    assert_eq!(
        reopened
            .checkpoint_goal_at(
                &owner,
                &id,
                1,
                checkpoint(&[], None, "stale"),
                "new-key",
                T0 + 4
            )
            .unwrap_err()
            .code(),
        "goal_revision_changed"
    );
}

#[test]
fn goal_plan_invalid_batch_is_atomic_and_completed_steps_never_regress() {
    let (_temp, db, owner) = fixture();
    let created = db.create_goal_at(&owner, input("create"), T0).unwrap();
    let id = created.goal.summary.goal_id;
    let first = db
        .checkpoint_goal_at(
            &owner,
            &id,
            1,
            checkpoint(&["inspect"], Some("implement"), "Ready"),
            "first",
            T0 + 1,
        )
        .unwrap();
    for (index, bad) in [
        checkpoint(
            &["verify", "missing"],
            None,
            "unknown must not partially complete verify",
        ),
        checkpoint(&["verify", "verify"], None, "duplicate"),
        checkpoint(&["bad id"], None, "invalid id"),
        checkpoint(&[], Some("missing"), "unknown current"),
        checkpoint(&[], Some("verify"), "two in progress"),
        checkpoint(&[], Some("inspect"), "regression"),
        checkpoint(&["verify"], Some("verify"), "contradictory batch"),
        checkpoint(&[], None, "  "),
        checkpoint(&[], None, &"x".repeat(MAX_GOAL_PROGRESS_SUMMARY_BYTES + 1)),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            db.checkpoint_goal_at(&owner, &id, 2, bad, &format!("bad-{index}"), T0 + 2)
                .unwrap_err()
                .code(),
            "invalid_goal_checkpoint"
        );
        assert_eq!(db.read_goal(&owner, &id).unwrap(), first.goal);
    }
    let second = db
        .checkpoint_goal_at(
            &owner,
            &id,
            2,
            checkpoint(&["implement"], Some("verify"), "Implement complete"),
            "second",
            T0 + 3,
        )
        .unwrap();
    assert_eq!(second.goal.summary.revision, 3);
    assert_eq!(second.goal.plan.completed_count(), 2);
    assert_eq!(second.goal.plan.current_step().unwrap().id, "verify");
    // Repeating an already completed selection cannot change its completion time.
    let third = db
        .checkpoint_goal_at(
            &owner,
            &id,
            3,
            checkpoint(&["inspect"], None, "Recovery checkpoint"),
            "third",
            T0 + 4,
        )
        .unwrap();
    assert_eq!(third.goal.summary.revision, 4);
    assert_eq!(third.goal.plan.steps[0], first.goal.plan.steps[0]);
}

#[test]
fn goal_plan_gates_completion_and_terminal_checkpoint_mutation() {
    let (_temp, db, owner) = fixture();
    let goal = db.create_goal_at(&owner, input("create"), T0).unwrap().goal;
    let id = &goal.summary.goal_id;
    assert_eq!(
        db.update_goal_at(
            &owner,
            id,
            1,
            GoalPatch {
                lifecycle: Some(GoalLifecycle::Completed),
                ..Default::default()
            },
            "complete",
            T0 + 1
        )
        .unwrap_err()
        .code(),
        "goal_plan_incomplete"
    );
    assert_eq!(db.read_goal(&owner, id).unwrap(), goal);
    let batch = checkpoint(
        &["inspect", "implement", "verify"],
        None,
        "Fresh validation and review complete",
    );
    let checkpointed = db
        .checkpoint_goal_at(&owner, id, 1, batch.clone(), "all", T0 + 2)
        .unwrap();
    assert!(checkpointed.goal.plan.complete());
    let completed = db
        .update_goal_at(
            &owner,
            id,
            2,
            GoalPatch {
                lifecycle: Some(GoalLifecycle::Completed),
                ..Default::default()
            },
            "complete-after-plan",
            T0 + 3,
        )
        .unwrap();
    assert_eq!(completed.goal.summary.revision, 3);
    assert_eq!(
        db.checkpoint_goal_at(
            &owner,
            id,
            3,
            checkpoint(&[], None, "not allowed"),
            "after-terminal",
            T0 + 4
        )
        .unwrap_err()
        .code(),
        "goal_terminal"
    );
    // Historical exact replay is not a mutation of terminal truth.
    assert!(
        db.checkpoint_goal_at(&owner, id, 1, batch, "all", T0 + 5)
            .unwrap()
            .replayed
    );
    assert_eq!(db.read_goal(&owner, id).unwrap(), completed.goal);
    let cancelled = db
        .create_goal_at(&owner, input("cancel-create"), T0)
        .unwrap()
        .goal;
    db.update_goal_at(
        &owner,
        &cancelled.summary.goal_id,
        1,
        GoalPatch {
            lifecycle: Some(GoalLifecycle::Cancelled),
            ..Default::default()
        },
        "cancel",
        T0 + 1,
    )
    .unwrap();
    assert_eq!(
        db.checkpoint_goal_at(
            &owner,
            &cancelled.summary.goal_id,
            2,
            checkpoint(&[], None, "no"),
            "cancelled",
            T0 + 2
        )
        .unwrap_err()
        .code(),
        "goal_terminal"
    );
}

#[test]
fn goal_plan_create_bounds_and_changed_key_are_enforced() {
    let (_temp, db, owner) = fixture();
    let mut bad_inputs = Vec::new();
    let mut bad = input("bounds");
    bad.steps[1].id = bad.steps[0].id.clone();
    bad_inputs.push(bad);
    let mut bad = input("bounds");
    bad.steps[0].id = "x".repeat(MAX_GOAL_STEP_ID_BYTES + 1);
    bad_inputs.push(bad);
    let mut bad = input("bounds");
    bad.steps[0].title = "x".repeat(MAX_GOAL_STEP_TITLE_CHARS + 1);
    bad_inputs.push(bad);
    let mut bad = input("bounds");
    bad.steps = (0..=MAX_GOAL_STEPS)
        .map(|i| NewGoalStep {
            id: format!("s{i}"),
            title: "step".into(),
        })
        .collect();
    bad_inputs.push(bad);
    let mut bad = input("bounds");
    bad.completion_conditions = vec!["condition".into(); MAX_GOAL_COMPLETION_CONDITIONS + 1];
    bad_inputs.push(bad);
    let mut bad = input("bounds");
    bad.completion_conditions = vec!["x".repeat(MAX_GOAL_CONDITION_BYTES + 1)];
    bad_inputs.push(bad);
    for bad in bad_inputs {
        assert_eq!(
            db.create_goal_at(&owner, bad, T0).unwrap_err().code(),
            "invalid_goal_plan"
        );
    }
    let created = db.create_goal_at(&owner, input("bounds"), T0).unwrap();
    assert_eq!(created.goal.summary.revision, 1);
    assert!(
        db.create_goal_at(&owner, input("bounds"), T0 + 1)
            .unwrap()
            .replayed
    );
    let mut changed = input("bounds");
    changed.steps[0].title.push_str(" changed");
    assert_eq!(
        db.create_goal_at(&owner, changed, T0 + 2)
            .unwrap_err()
            .code(),
        "goal_idempotency_conflict"
    );
}

#[test]
fn goal_plan_malformed_persistence_fails_closed_for_reads_lists_and_mutations() {
    let (_temp, db, owner) = fixture();
    let goal = db.create_goal_at(&owner, input("create"), T0).unwrap().goal;
    let mut malformed = vec![
        "{}".to_string(),
        "null".into(),
        "not json".into(),
        "x".repeat(MAX_GOAL_PLAN_BYTES + 1),
    ];
    for mutation in ["duplicate", "status", "two_current", "timestamp", "summary"] {
        let mut value = serde_json::to_value(&goal.plan).unwrap();
        match mutation {
            "duplicate" => value["steps"][1]["id"] = value["steps"][0]["id"].clone(),
            "status" => value["steps"][0]["status"] = "unknown".into(),
            "two_current" => {
                value["steps"][0]["status"] = "in_progress".into();
                value["steps"][1]["status"] = "in_progress".into();
            }
            "timestamp" => value["steps"][0]["updated_at_unix_ms"] = (T0 + 1).into(),
            "summary" => value["progress_summary"] = "missing checkpoint time".into(),
            _ => unreachable!(),
        }
        malformed.push(value.to_string());
    }
    for raw in malformed {
        db.lock_connection(crate::StoreDomain::Goal)
            .execute(
                "UPDATE wc_goals SET plan_json = ?2 WHERE goal_id = ?1",
                params![goal.summary.goal_id, raw],
            )
            .unwrap();
        assert_eq!(
            db.read_goal(&owner, &goal.summary.goal_id)
                .unwrap_err()
                .code(),
            "goal_store_unavailable"
        );
        assert_eq!(
            db.list_goals(&owner, None, None, 0, 10).unwrap_err().code(),
            "goal_store_unavailable"
        );
        assert_eq!(
            db.checkpoint_goal_at(
                &owner,
                &goal.summary.goal_id,
                1,
                checkpoint(&[], None, "must not repair corruption"),
                "bad-store",
                T0 + 2
            )
            .unwrap_err()
            .code(),
            "goal_store_unavailable"
        );
    }
}

#[test]
fn goal_plan_revision_fence_serializes_competing_checkpoints() {
    let (_temp, db, owner) = fixture();
    let id = db
        .create_goal_at(&owner, input("create"), T0)
        .unwrap()
        .goal
        .summary
        .goal_id;
    let db = std::sync::Arc::new(db);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|i| {
            let (db, owner, id, barrier) = (db.clone(), owner.clone(), id.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                db.checkpoint_goal_at(
                    &owner,
                    &id,
                    1,
                    checkpoint(&[], Some("inspect"), "Start"),
                    &format!("race-{i}"),
                    T0 + 1,
                )
            })
        })
        .collect();
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .find_map(|result| result.as_ref().err())
            .unwrap()
            .code(),
        "goal_revision_changed"
    );
    assert_eq!(db.read_goal(&owner, &id).unwrap().summary.revision, 2);
}

#[test]
fn goal_plan_persisted_progress_requires_checkpoint_and_monotonic_step_time() {
    let (_temp, db, owner) = fixture();
    let goal = db
        .create_goal_at(&owner, input("progress-shape"), T0)
        .unwrap()
        .goal;
    for (status, step_time, checkpoint_time) in [
        ("completed", T0, None),
        ("in_progress", T0, None),
        ("pending", T0 + 1, Some(T0 + 2)),
        ("completed", T0 + 2, Some(T0 + 1)),
    ] {
        let mut value = serde_json::to_value(&goal.plan).unwrap();
        value["steps"][0]["status"] = status.into();
        value["steps"][0]["updated_at_unix_ms"] = step_time.into();
        if let Some(at) = checkpoint_time {
            value["progress_summary"] = "checkpoint".into();
            value["checkpoint_at_unix_ms"] = at.into();
        }
        db.lock_connection(crate::StoreDomain::Goal)
            .execute(
                "UPDATE wc_goals SET plan_json = ?2, updated_at_unix_ms = ?3 WHERE goal_id = ?1",
                params![goal.summary.goal_id, value.to_string(), T0 + 3],
            )
            .unwrap();
        assert_eq!(
            db.read_goal(&owner, &goal.summary.goal_id)
                .unwrap_err()
                .code(),
            "goal_store_unavailable"
        );
    }
}

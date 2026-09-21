use super::communication::{
    CommunicationPrincipal, NewAgentIdentity, COMMUNICATION_PRINCIPAL_DIGEST_PREFIX,
    DURABLE_AGENT_ID_PREFIX,
};
use super::goal::*;
use super::Database;

const T0: i64 = 10_000;

fn principal(hex: char) -> CommunicationPrincipal {
    CommunicationPrincipal {
        kind: "user".to_string(),
        digest: format!(
            "{COMMUNICATION_PRINCIPAL_DIGEST_PREFIX}{}",
            hex.to_string().repeat(64)
        ),
    }
}

fn create_agent(db: &Database, owner: &CommunicationPrincipal, key: &str) -> String {
    db.create_agent_identity(
        owner,
        NewAgentIdentity {
            handle: key.to_string(),
            display_name: key.to_string(),
            description: String::new(),
            specialty_labels: Vec::new(),
            idempotency_key: format!("agent-{key}"),
        },
    )
    .unwrap()
    .agent
    .agent_id
}

fn input(key: &str) -> NewGoal {
    NewGoal {
        completion_conditions: Vec::new(),
        steps: Vec::new(),
        title: "Ship durable Goal foundation".to_string(),
        objective: "Preserve high-level durable intent without granting execution authority."
            .to_string(),
        controller_agent_id: None,
        idempotency_key: key.to_string(),
    }
}

#[test]
fn create_read_list_update_and_terminal_replay_are_durable_and_revisioned() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("goals.db");
    let owner = principal('a');
    let goal_id = {
        let db = Database::open(&path).unwrap();
        let created = db.create_goal_at(&owner, input("create-goal"), T0).unwrap();
        assert!(created.created);
        assert!(!created.replayed);
        assert!(created.state_changed);
        assert!(created.goal.summary.goal_id.starts_with(GOAL_ID_PREFIX));
        assert_eq!(created.goal.summary.lifecycle, GoalLifecycle::Active);
        assert_eq!(created.goal.summary.revision, 1);
        assert!(created.goal.correlations.is_empty());

        let replay = db
            .create_goal_at(&owner, input("create-goal"), T0 + 1)
            .unwrap();
        assert!(replay.replayed);
        assert!(!replay.state_changed);
        assert_eq!(replay.goal.summary.goal_id, created.goal.summary.goal_id);

        let mut changed_create = input("create-goal");
        changed_create.objective.push_str(" Changed.");
        let conflict = db
            .create_goal_at(&owner, changed_create, T0 + 2)
            .unwrap_err();
        assert_eq!(conflict.code(), "goal_idempotency_conflict");

        let updated = db
            .update_goal_at(
                &owner,
                &created.goal.summary.goal_id,
                1,
                GoalPatch {
                    title: Some("Ship Goal foundation".to_string()),
                    objective: None,
                    controller_agent_id: None,
                    lifecycle: None,
                    terminal_reason: None,
                },
                "update-title",
                T0 + 3,
            )
            .unwrap();
        assert!(updated.state_changed);
        assert_eq!(updated.goal.summary.revision, 2);
        assert_eq!(updated.goal.summary.lifecycle, GoalLifecycle::Active);

        let changed_update_reuse = db
            .update_goal_at(
                &owner,
                &created.goal.summary.goal_id,
                1,
                GoalPatch {
                    title: Some("Different title under same key".to_string()),
                    objective: None,
                    controller_agent_id: None,
                    lifecycle: None,
                    terminal_reason: None,
                },
                "update-title",
                T0 + 3,
            )
            .unwrap_err();
        assert_eq!(changed_update_reuse.code(), "goal_idempotency_conflict");

        let terminal = db
            .update_goal_at(
                &owner,
                &created.goal.summary.goal_id,
                2,
                GoalPatch {
                    title: None,
                    objective: None,
                    controller_agent_id: None,
                    lifecycle: Some(GoalLifecycle::Completed),
                    terminal_reason: Some("Phase 1 accepted".to_string()),
                },
                "complete-goal",
                T0 + 4,
            )
            .unwrap();
        assert_eq!(terminal.goal.summary.lifecycle, GoalLifecycle::Completed);
        assert_eq!(terminal.goal.summary.revision, 3);
        assert_eq!(terminal.goal.summary.terminal_at_unix_ms, Some(T0 + 4));
        assert_eq!(
            terminal.goal.terminal_reason.as_deref(),
            Some("Phase 1 accepted")
        );

        let terminal_replay = db
            .update_goal_at(
                &owner,
                &created.goal.summary.goal_id,
                2,
                GoalPatch {
                    title: None,
                    objective: None,
                    controller_agent_id: None,
                    lifecycle: Some(GoalLifecycle::Completed),
                    terminal_reason: Some("Phase 1 accepted".to_string()),
                },
                "complete-goal",
                T0 + 5,
            )
            .unwrap();
        assert!(terminal_replay.replayed);
        assert!(!terminal_replay.state_changed);
        assert_eq!(terminal_replay.goal.summary.revision, 3);

        let page = db
            .list_goals(&owner, Some(GoalLifecycle::Completed), 0, 10)
            .unwrap();
        assert_eq!(page.total_count, 1);
        assert_eq!(page.goals[0].goal_id, created.goal.summary.goal_id);
        assert_eq!(page.goals[0].agent_task_count, 0);
        assert_eq!(page.goals[0].workflow_session_count, 0);
        created.goal.summary.goal_id
    };

    let reopened = Database::open(&path).unwrap();
    let goal = reopened.read_goal(&owner, &goal_id).unwrap();
    assert_eq!(goal.summary.lifecycle, GoalLifecycle::Completed);
    assert_eq!(goal.summary.revision, 3);
    assert_eq!(goal.terminal_reason.as_deref(), Some("Phase 1 accepted"));
}

#[test]
fn exact_read_hides_foreign_existence_and_updates_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("goal-auth.db")).unwrap();
    let owner = principal('b');
    let foreign = principal('c');
    let created = db
        .create_goal_at(&owner, input("owner-create"), T0)
        .unwrap();
    let goal_id = created.goal.summary.goal_id;

    let foreign_error = db.read_goal(&foreign, &goal_id).unwrap_err();
    assert_eq!(foreign_error.code(), "goal_not_found");
    let missing = format!("{GOAL_ID_PREFIX}{}", "f".repeat(16));
    let missing_error = db.read_goal(&foreign, &missing).unwrap_err();
    assert_eq!(missing_error.code(), "goal_not_found");

    let stale = db
        .update_goal_at(
            &owner,
            &goal_id,
            99,
            GoalPatch {
                title: Some("stale".to_string()),
                ..GoalPatch::default()
            },
            "stale-update",
            T0 + 1,
        )
        .unwrap_err();
    assert_eq!(stale.code(), "goal_revision_changed");
    assert_eq!(stale.current_revision(), Some(1));

    let terminal = db
        .update_goal_at(
            &owner,
            &goal_id,
            1,
            GoalPatch {
                lifecycle: Some(GoalLifecycle::Cancelled),
                ..GoalPatch::default()
            },
            "cancel-goal",
            T0 + 2,
        )
        .unwrap();
    assert_eq!(terminal.goal.summary.lifecycle, GoalLifecycle::Cancelled);
    let immutable = db
        .update_goal_at(
            &owner,
            &goal_id,
            2,
            GoalPatch {
                title: Some("cannot mutate".to_string()),
                ..GoalPatch::default()
            },
            "terminal-mutate",
            T0 + 3,
        )
        .unwrap_err();
    assert_eq!(immutable.code(), "goal_terminal");
}

#[test]
fn explicit_controller_is_authorized_revisioned_replayed_and_persisted() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("goal-controller.db");
    let owner = principal('8');
    let foreign = principal('9');
    let (goal_id, controller_b) = {
        let db = Database::open(&path).unwrap();
        let controller_a = create_agent(&db, &owner, "goal-controller-a");
        let controller_b = create_agent(&db, &owner, "goal-controller-b");
        let foreign_controller = create_agent(&db, &foreign, "foreign-goal-controller");
        let missing_controller = format!("{DURABLE_AGENT_ID_PREFIX}{}", "z".repeat(16));

        let mut create = input("controller-create");
        create.controller_agent_id = Some(controller_a.clone());
        let created = db.create_goal_at(&owner, create.clone(), T0).unwrap();
        let goal_id = created.goal.summary.goal_id.clone();
        assert_eq!(
            created.goal.controller_agent_id.as_deref(),
            Some(controller_a.as_str())
        );
        assert_eq!(created.goal.summary.revision, 1);

        let replay = db.create_goal_at(&owner, create, T0 + 1).unwrap();
        assert!(replay.replayed);
        assert!(!replay.state_changed);
        assert_eq!(replay.goal.controller_agent_id, Some(controller_a.clone()));

        let mut changed_create = input("controller-create");
        changed_create.controller_agent_id = Some(controller_b.clone());
        assert_eq!(
            db.create_goal_at(&owner, changed_create, T0 + 2)
                .unwrap_err()
                .code(),
            "goal_idempotency_conflict"
        );

        let mut foreign_create = input("foreign-controller-create");
        foreign_create.controller_agent_id = Some(foreign_controller.clone());
        let foreign_error = db
            .create_goal_at(&owner, foreign_create, T0 + 3)
            .unwrap_err();
        let mut missing_create = input("missing-controller-create");
        missing_create.controller_agent_id = Some(missing_controller.clone());
        let missing_error = db
            .create_goal_at(&owner, missing_create, T0 + 4)
            .unwrap_err();
        assert_eq!(foreign_error.code(), "agent_not_found");
        assert_eq!(missing_error.code(), foreign_error.code());
        assert_eq!(missing_error.message(), foreign_error.message());

        let updated = db
            .update_goal_at(
                &owner,
                &goal_id,
                1,
                GoalPatch {
                    controller_agent_id: Some(controller_b.clone()),
                    ..GoalPatch::default()
                },
                "set-controller-b",
                T0 + 5,
            )
            .unwrap();
        assert_eq!(updated.goal.summary.revision, 2);
        assert_eq!(
            updated.goal.controller_agent_id.as_deref(),
            Some(controller_b.as_str())
        );

        let replay = db
            .update_goal_at(
                &owner,
                &goal_id,
                1,
                GoalPatch {
                    controller_agent_id: Some(controller_b.clone()),
                    ..GoalPatch::default()
                },
                "set-controller-b",
                T0 + 6,
            )
            .unwrap();
        assert!(replay.replayed);
        assert!(!replay.state_changed);
        assert_eq!(replay.goal.summary.revision, 2);

        let foreign_update = db
            .update_goal_at(
                &owner,
                &goal_id,
                2,
                GoalPatch {
                    controller_agent_id: Some(foreign_controller.clone()),
                    ..GoalPatch::default()
                },
                "foreign-controller-update",
                T0 + 7,
            )
            .unwrap_err();
        let missing_update = db
            .update_goal_at(
                &owner,
                &goal_id,
                2,
                GoalPatch {
                    controller_agent_id: Some(missing_controller),
                    ..GoalPatch::default()
                },
                "missing-controller-update",
                T0 + 8,
            )
            .unwrap_err();
        assert_eq!(foreign_update.code(), "agent_not_found");
        assert_eq!(missing_update.code(), foreign_update.code());
        assert_eq!(missing_update.message(), foreign_update.message());

        let foreign_goal_probe = db
            .update_goal_at(
                &foreign,
                &goal_id,
                2,
                GoalPatch {
                    controller_agent_id: Some(foreign_controller),
                    ..GoalPatch::default()
                },
                "foreign-goal-controller-probe",
                T0 + 9,
            )
            .unwrap_err();
        assert_eq!(foreign_goal_probe.code(), "goal_not_found");

        let terminal = db
            .update_goal_at(
                &owner,
                &goal_id,
                2,
                GoalPatch {
                    lifecycle: Some(GoalLifecycle::Completed),
                    ..GoalPatch::default()
                },
                "controller-goal-terminal",
                T0 + 10,
            )
            .unwrap();
        assert_eq!(terminal.goal.summary.revision, 3);
        assert_eq!(
            terminal.goal.controller_agent_id.as_deref(),
            Some(controller_b.as_str())
        );
        assert_eq!(
            db.update_goal_at(
                &owner,
                &goal_id,
                3,
                GoalPatch {
                    controller_agent_id: Some(controller_a),
                    ..GoalPatch::default()
                },
                "terminal-controller-change",
                T0 + 11,
            )
            .unwrap_err()
            .code(),
            "goal_terminal"
        );
        (goal_id, controller_b)
    };

    let reopened = Database::open(&path).unwrap();
    let persisted = reopened.read_goal(&owner, &goal_id).unwrap();
    assert_eq!(persisted.summary.lifecycle, GoalLifecycle::Completed);
    assert_eq!(persisted.summary.revision, 3);
    assert_eq!(
        persisted.controller_agent_id.as_deref(),
        Some(controller_b.as_str())
    );
}

#[test]
fn prepare_goal_workflow_is_atomic_revision_one_owned_controller_and_keyed() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("goal-prepare-workflow.db")).unwrap();
    let owner = principal('c');
    let foreign = principal('f');
    let controller = create_agent(&db, &owner, "prepare-controller");
    let foreign_controller = create_agent(&db, &foreign, "prepare-foreign-controller");
    let session_id = format!("wc_sess_{}", "3".repeat(32));

    let mut prepared_input = input("prepare-workflow");
    prepared_input.controller_agent_id = Some(controller.clone());
    let prepared = db
        .prepare_goal_workflow_at(&owner, &session_id, prepared_input.clone(), T0)
        .unwrap();
    assert!(prepared.created);
    assert!(!prepared.replayed);
    assert!(prepared.state_changed);
    assert_eq!(prepared.goal.summary.revision, 1);
    assert_eq!(prepared.goal.summary.workflow_session_count, 1);
    assert_eq!(prepared.goal.summary.agent_task_count, 0);
    assert_eq!(
        prepared.goal.controller_agent_id.as_deref(),
        Some(controller.as_str())
    );
    assert_eq!(prepared.goal.correlations.len(), 1);
    assert_eq!(
        prepared.goal.correlations[0].kind,
        GoalCorrelationKind::WorkflowSession
    );
    assert_eq!(prepared.goal.correlations[0].reference_id, session_id);

    let replay = db
        .prepare_goal_workflow_at(&owner, &session_id, prepared_input.clone(), T0 + 1)
        .unwrap();
    assert!(!replay.created);
    assert!(replay.replayed);
    assert!(!replay.state_changed);
    assert_eq!(replay.goal.summary.goal_id, prepared.goal.summary.goal_id);
    assert_eq!(replay.goal.summary.revision, 1);
    assert_eq!(replay.goal.summary.workflow_session_count, 1);

    let changed_session = format!("wc_sess_{}", "4".repeat(32));
    assert_eq!(
        db.prepare_goal_workflow_at(&owner, &changed_session, prepared_input.clone(), T0 + 2)
            .unwrap_err()
            .code(),
        "goal_idempotency_conflict"
    );
    let mut changed_request = prepared_input.clone();
    changed_request.objective.push_str(" changed");
    assert_eq!(
        db.prepare_goal_workflow_at(&owner, &session_id, changed_request, T0 + 3)
            .unwrap_err()
            .code(),
        "goal_idempotency_conflict"
    );

    let before_failures = db.list_goals(&owner, None, 0, 100).unwrap().total_count;
    assert!(db
        .prepare_goal_workflow_at(&owner, "not-a-session", input("invalid-session"), T0 + 4)
        .is_err());
    assert_eq!(
        db.list_goals(&owner, None, 0, 100).unwrap().total_count,
        before_failures
    );

    let mut foreign_input = input("foreign-controller-prepare");
    foreign_input.controller_agent_id = Some(foreign_controller);
    let foreign_error = db
        .prepare_goal_workflow_at(&owner, &session_id, foreign_input, T0 + 5)
        .unwrap_err();
    let mut missing_input = input("missing-controller-prepare");
    missing_input.controller_agent_id = Some("wc_dagent_________________".to_string());
    let missing_error = db
        .prepare_goal_workflow_at(&owner, &session_id, missing_input, T0 + 6)
        .unwrap_err();
    assert_eq!(foreign_error.code(), "agent_not_found");
    assert_eq!(missing_error.code(), foreign_error.code());
    assert_eq!(missing_error.message(), foreign_error.message());
    assert_eq!(
        db.list_goals(&owner, None, 0, 100).unwrap().total_count,
        before_failures
    );

    {
        let conn = db.conn_for_tests();
        conn.execute_batch(
            "CREATE TRIGGER fail_prepare_goal_correlation
             BEFORE INSERT ON wc_goal_correlations
             BEGIN SELECT RAISE(ABORT, 'test injected prepare correlation failure'); END;",
        )
        .unwrap();
    }
    let correlation_failure = db
        .prepare_goal_workflow_at(
            &owner,
            &format!("wc_sess_{}", "5".repeat(32)),
            input("prepare-correlation-failure"),
            T0 + 7,
        )
        .unwrap_err();
    assert_eq!(correlation_failure.code(), "goal_store_unavailable");
    {
        let conn = db.conn_for_tests();
        conn.execute_batch("DROP TRIGGER fail_prepare_goal_correlation;")
            .unwrap();
    }
    assert_eq!(
        db.list_goals(&owner, None, 0, 100).unwrap().total_count,
        before_failures
    );

    {
        let conn = db.conn_for_tests();
        conn.execute_batch(
            "CREATE TRIGGER fail_prepare_goal_idempotency
             BEFORE INSERT ON wc_goal_idempotency
             WHEN NEW.operation = 'prepare_goal_workflow'
             BEGIN SELECT RAISE(ABORT, 'test injected prepare idempotency failure'); END;",
        )
        .unwrap();
    }
    let idempotency_failure = db
        .prepare_goal_workflow_at(
            &owner,
            &format!("wc_sess_{}", "6".repeat(32)),
            input("prepare-idempotency-failure"),
            T0 + 8,
        )
        .unwrap_err();
    assert_eq!(idempotency_failure.code(), "goal_store_unavailable");
    {
        let conn = db.conn_for_tests();
        conn.execute_batch("DROP TRIGGER fail_prepare_goal_idempotency;")
            .unwrap();
        let orphan_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM wc_goal_correlations c
                 LEFT JOIN wc_goals g ON g.goal_id = c.goal_id
                 WHERE g.goal_id IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(orphan_count, 0);
    }
    assert_eq!(
        db.list_goals(&owner, None, 0, 100).unwrap().total_count,
        before_failures
    );

    let omitted = db
        .prepare_goal_workflow_at(
            &owner,
            &format!("wc_sess_{}", "7".repeat(32)),
            input("prepare-no-controller"),
            T0 + 9,
        )
        .unwrap();
    assert!(omitted.goal.controller_agent_id.is_none());
    assert_eq!(omitted.goal.summary.revision, 1);
    assert_eq!(omitted.goal.summary.workflow_session_count, 1);
}

#[test]
fn correlations_are_bounded_explicit_identity_only_and_replayed() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("goal-correlations.db")).unwrap();
    let owner = principal('d');
    let created = db.create_goal_at(&owner, input("corr-create"), T0).unwrap();
    let goal_id = created.goal.summary.goal_id;
    let task_id = "wc_agent_task_ERERERERERERERER".to_string();
    let session_id = format!("wc_sess_{}", "2".repeat(32));

    let task_link = db
        .associate_goal_reference_at(
            &owner,
            &goal_id,
            GoalCorrelationKind::AgentTask,
            &task_id,
            "task-link",
            T0 + 1,
        )
        .unwrap();
    assert!(task_link.state_changed);
    assert_eq!(task_link.goal.summary.revision, 2);
    assert_eq!(task_link.goal.summary.agent_task_count, 1);

    let task_replay = db
        .associate_goal_reference_at(
            &owner,
            &goal_id,
            GoalCorrelationKind::AgentTask,
            &task_id,
            "task-link",
            T0 + 2,
        )
        .unwrap();
    assert!(task_replay.replayed);
    assert!(!task_replay.state_changed);

    let session_link = db
        .associate_goal_reference_at(
            &owner,
            &goal_id,
            GoalCorrelationKind::WorkflowSession,
            &session_id,
            "session-link",
            T0 + 3,
        )
        .unwrap();
    assert_eq!(session_link.goal.summary.revision, 3);
    assert_eq!(session_link.goal.summary.workflow_session_count, 1);
    assert_eq!(session_link.goal.correlations.len(), 2);

    let changed_reuse = db
        .associate_goal_reference_at(
            &owner,
            &goal_id,
            GoalCorrelationKind::WorkflowSession,
            &format!("wc_sess_{}", webcodex_core::compact::random_suffix::<12>()),
            "session-link",
            T0 + 4,
        )
        .unwrap_err();
    assert_eq!(changed_reuse.code(), "goal_idempotency_conflict");

    for ordinal in 0..(MAX_GOAL_CORRELATIONS - 2) {
        let reference_id = format!(
            "wc_agent_task_{}",
            webcodex_core::compact::encode(&(ordinal as u128).to_be_bytes()[4..])
        );
        let key = format!("capacity-link-{ordinal}");
        db.associate_goal_reference_at(
            &owner,
            &goal_id,
            GoalCorrelationKind::AgentTask,
            &reference_id,
            &key,
            T0 + 10 + ordinal,
        )
        .unwrap();
    }
    let full = db.read_goal(&owner, &goal_id).unwrap();
    assert_eq!(full.correlations.len(), MAX_GOAL_CORRELATIONS as usize);
    let overflow = db
        .associate_goal_reference_at(
            &owner,
            &goal_id,
            GoalCorrelationKind::AgentTask,
            &"wc_agent_task_________________".to_string(),
            "capacity-overflow",
            T0 + 100,
        )
        .unwrap_err();
    assert_eq!(overflow.code(), "goal_correlation_capacity_exceeded");
}

#[test]
fn bounds_and_unknown_persisted_lifecycle_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("goal-bounds.db")).unwrap();
    let owner = principal('e');

    let mut oversized = input("oversized-title");
    oversized.title = "x".repeat(MAX_GOAL_TITLE_CHARS + 1);
    assert_eq!(
        db.create_goal_at(&owner, oversized, T0).unwrap_err().code(),
        "invalid_goal_title"
    );
    let mut oversized = input("oversized-objective");
    oversized.objective = "x".repeat(MAX_GOAL_OBJECTIVE_BYTES + 1);
    assert_eq!(
        db.create_goal_at(&owner, oversized, T0).unwrap_err().code(),
        "invalid_goal_objective"
    );
    assert_eq!(
        db.list_goals(&owner, None, 0, MAX_GOAL_LIST_LIMIT + 1)
            .unwrap_err()
            .code(),
        "invalid_goal_list_limit"
    );
    #[cfg(target_pointer_width = "64")]
    assert_eq!(
        db.list_goals(&owner, None, (i64::MAX as usize) + 1, 1)
            .unwrap_err()
            .code(),
        "invalid_goal_list_offset"
    );

    let oversized_persisted = db
        .create_goal_at(&owner, input("corrupt-objective-create"), T0)
        .unwrap();
    {
        let conn = db.conn_for_tests();
        conn.execute(
            "UPDATE wc_goals SET objective = ?2 WHERE goal_id = ?1",
            rusqlite::params![
                oversized_persisted.goal.summary.goal_id,
                "x".repeat(MAX_GOAL_OBJECTIVE_BYTES + 1)
            ],
        )
        .unwrap();
    }
    let error = db
        .read_goal(&owner, &oversized_persisted.goal.summary.goal_id)
        .unwrap_err();
    assert_eq!(error.code(), "goal_store_unavailable");

    let overlinked = db
        .create_goal_at(&owner, input("corrupt-correlations-create"), T0)
        .unwrap();
    {
        let conn = db.conn_for_tests();
        for ordinal in 0..=MAX_GOAL_CORRELATIONS {
            conn.execute(
                "INSERT INTO wc_goal_correlations (
                    goal_id, kind, reference_id, created_at_unix_ms
                 ) VALUES (?1, 'agent_task', ?2, ?3)",
                rusqlite::params![
                    overlinked.goal.summary.goal_id,
                    format!(
                        "wc_agent_task_{}",
                        webcodex_core::compact::encode(&(ordinal as u128).to_be_bytes()[4..])
                    ),
                    T0 + ordinal,
                ],
            )
            .unwrap();
        }
    }
    let error = db
        .read_goal(&owner, &overlinked.goal.summary.goal_id)
        .unwrap_err();
    assert_eq!(error.code(), "goal_store_unavailable");

    let created = db
        .create_goal_at(&owner, input("corrupt-create"), T0)
        .unwrap();
    let goal_id = created.goal.summary.goal_id;
    {
        let conn = db.conn_for_tests();
        conn.execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();
        conn.execute(
            "UPDATE wc_goals SET lifecycle = 'waiting_validation' WHERE goal_id = ?1",
            [goal_id.as_str()],
        )
        .unwrap();
        conn.execute_batch("PRAGMA ignore_check_constraints = OFF;")
            .unwrap();
    }
    let error = db.read_goal(&owner, &goal_id).unwrap_err();
    assert_eq!(error.code(), "goal_store_unavailable");
}

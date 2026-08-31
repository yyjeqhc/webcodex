use super::agent_task::*;
use super::communication::{
    CommunicationPrincipal, NewAgentEndpoint, NewAgentIdentity, NewConversation,
    NewConversationMessage, COMMUNICATION_PRINCIPAL_DIGEST_PREFIX,
};
use super::Database;
use rusqlite::params;
use std::sync::{mpsc, Arc, Barrier};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const T0: i64 = 1_000_000;

fn wall_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("wall clock after Unix epoch")
        .as_millis()
        .try_into()
        .expect("wall-clock milliseconds fit i64")
}

fn principal(hex: char) -> CommunicationPrincipal {
    CommunicationPrincipal {
        kind: "user".to_string(),
        digest: format!(
            "{COMMUNICATION_PRINCIPAL_DIGEST_PREFIX}{}",
            hex.to_string().repeat(64)
        ),
    }
}

fn agent(db: &Database, owner: &CommunicationPrincipal, label: &str) -> String {
    db.create_agent_identity(
        owner,
        NewAgentIdentity {
            handle: label.to_string(),
            display_name: label.to_string(),
            description: String::new(),
            specialty_labels: Vec::new(),
            idempotency_key: format!("create-{label}"),
        },
    )
    .unwrap()
    .agent
    .agent_id
}

fn task_input(assignee_agent_id: Option<String>, key: &str) -> NewAgentTask {
    NewAgentTask {
        title: "Durable work".to_string(),
        instruction: "Perform bounded durable work without assuming a window or Endpoint."
            .to_string(),
        assignee_agent_id,
        source_conversation_id: None,
        source_message_id: None,
        referenced_project_id: Some("agent:special:reference-only".to_string()),
        idempotency_key: key.to_string(),
    }
}

fn create_assigned_task(
    db: &Database,
    owner: &CommunicationPrincipal,
    assignee: &str,
    key: &str,
) -> String {
    db.create_agent_task_at(owner, task_input(Some(assignee.to_string()), key), T0)
        .unwrap()
        .task
        .summary
        .task_id
}

fn start(
    db: &Database,
    owner: &CommunicationPrincipal,
    task_id: &str,
    assignee: &str,
    key: &str,
    now: i64,
) -> AgentTaskAttemptStartMutation {
    db.start_agent_task_attempt_at(owner, task_id, assignee, key, now)
        .unwrap()
}

#[test]
fn agent_task_create_is_keyed_bounded_and_restart_durable() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("agent-task-create.db");
    let db = Database::open(&path).unwrap();
    let owner = principal('1');
    let assignee = agent(&db, &owner, "creator-agent");
    let created = db
        .create_agent_task_at(
            &owner,
            task_input(Some(assignee.clone()), "task-create-key"),
            T0,
        )
        .unwrap();
    assert!(created.created);
    assert!(!created.replayed);
    assert!(created.state_changed);
    assert!(created
        .task
        .summary
        .task_id
        .starts_with(AGENT_TASK_ID_PREFIX));
    assert_eq!(created.task.summary.state, AgentTaskState::Ready);
    assert_eq!(
        created.task.summary.assignee_agent_id.as_deref(),
        Some(assignee.as_str())
    );
    assert_eq!(
        created.task.summary.referenced_project_id.as_deref(),
        Some("agent:special:reference-only")
    );
    assert!(created.task.summary.latest_attempt.is_none());

    let replay = db
        .create_agent_task_at(
            &owner,
            task_input(Some(assignee.clone()), "task-create-key"),
            T0 + 1,
        )
        .unwrap();
    assert_eq!(replay.task.summary.task_id, created.task.summary.task_id);
    assert!(replay.replayed);
    assert!(!replay.state_changed);

    let mut changed = task_input(Some(assignee), "task-create-key");
    changed.instruction.push_str(" changed");
    let conflict = db
        .create_agent_task_at(&owner, changed, T0 + 2)
        .unwrap_err();
    assert_eq!(conflict.code(), "communication_idempotency_conflict");

    let task_id = created.task.summary.task_id.clone();
    drop(db);
    let reopened = Database::open(&path).unwrap();
    let recovered = reopened.read_agent_task(&owner, &task_id).unwrap();
    assert_eq!(recovered.summary.task_id, task_id);
    assert_eq!(recovered.summary.state, AgentTaskState::Ready);
    assert_eq!(recovered.instruction, created.task.instruction);
}

#[test]
fn unassigned_and_foreign_assignee_tasks_fail_closed_until_explicit_assignment() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("agent-task-assignment.db")).unwrap();
    let owner = principal('2');
    let foreign = principal('3');
    let owned_agent = agent(&db, &owner, "owned-assignee");
    let foreign_agent = agent(&db, &foreign, "foreign-assignee");

    let unassigned = db
        .create_agent_task_at(&owner, task_input(None, "unassigned"), T0)
        .unwrap();
    let task_id = unassigned.task.summary.task_id;
    let error = db
        .start_agent_task_attempt_at(
            &owner,
            &task_id,
            &owned_agent,
            "start-before-assign",
            T0 + 10,
        )
        .unwrap_err();
    assert_eq!(error.code(), "agent_task_unassigned");

    let foreign_error = db
        .assign_agent_task_at(&owner, &task_id, &foreign_agent, T0 + 11)
        .unwrap_err();
    assert_eq!(foreign_error.code(), "agent_not_found");

    let foreign_create = db
        .create_agent_task_at(
            &owner,
            task_input(Some(foreign_agent), "foreign-create-assignee"),
            T0 + 11,
        )
        .unwrap_err();
    assert_eq!(foreign_create.code(), "agent_not_found");

    let assigned = db
        .assign_agent_task_at(&owner, &task_id, &owned_agent, T0 + 12)
        .unwrap();
    assert!(assigned.state_changed);
    assert_eq!(
        assigned.task.summary.assignee_agent_id.as_deref(),
        Some(owned_agent.as_str())
    );
    let attempt = start(
        &db,
        &owner,
        &task_id,
        &owned_agent,
        "start-after-assign",
        T0 + 13,
    );
    assert_eq!(attempt.attempt.attempt_number, 1);
}

#[test]
fn concurrent_attempt_start_creates_exactly_one_authoritative_attempt() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("agent-task-race.db");
    let setup = Database::open(&path).unwrap();
    let owner = principal('4');
    let assignee = agent(&setup, &owner, "race-agent");
    let task_id = create_assigned_task(&setup, &owner, &assignee, "race-task");
    drop(setup);

    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();
    for suffix in ["a", "b"] {
        let path = path.clone();
        let barrier = barrier.clone();
        let owner = owner.clone();
        let assignee = assignee.clone();
        let task_id = task_id.clone();
        handles.push(std::thread::spawn(move || {
            let db = Database::open(&path).unwrap();
            barrier.wait();
            db.start_agent_task_attempt_at(
                &owner,
                &task_id,
                &assignee,
                &format!("race-start-{suffix}"),
                T0 + 100,
            )
        }));
    }
    let results = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter_map(|result| result.as_ref().err())
            .map(|error| error.code())
            .collect::<Vec<_>>(),
        vec!["agent_task_attempt_active"]
    );

    let reopened = Database::open(&path).unwrap();
    let count: i64 = reopened
        .conn_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM wc_agent_task_attempts WHERE task_id = ?1",
            [task_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn live_lease_mutations_sample_server_time_after_serialization_wait() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("agent-task-serialized-clock.db");
    let db = Arc::new(Database::open(&path).unwrap());
    let owner = principal('e');
    let assignee = agent(&db, &owner, "serialized-clock-agent");

    let heartbeat_task = create_assigned_task(&db, &owner, &assignee, "heartbeat-clock-task");
    let heartbeat_attempt = start(
        &db,
        &owner,
        &heartbeat_task,
        &assignee,
        "heartbeat-clock-start",
        T0 + 15,
    );
    let completion_task = create_assigned_task(&db, &owner, &assignee, "completion-clock-task");
    let completion_attempt = start(
        &db,
        &owner,
        &completion_task,
        &assignee,
        "completion-clock-start",
        T0 + 16,
    );

    // Hold the same Database mutex that production mutations serialize through.
    // Workers announce immediately before entering the public mutation wrapper. The
    // lease is then moved to a point after any pre-lock clock sample but before the
    // mutex is released. A wrapper that sampled time before serialization would
    // incorrectly renew/complete; the production path must sample after it acquires
    // the authoritative transaction.
    let guard = db.conn_for_tests();
    let (ready_tx, ready_rx) = mpsc::channel();

    let heartbeat_db = Arc::clone(&db);
    let heartbeat_owner = owner.clone();
    let heartbeat_assignee = assignee.clone();
    let heartbeat_task_id = heartbeat_task.clone();
    let heartbeat_attempt_id = heartbeat_attempt.attempt.attempt_id.clone();
    let heartbeat_fence = heartbeat_attempt.attempt_fence.clone();
    let heartbeat_ready = ready_tx.clone();
    let heartbeat_handle = std::thread::spawn(move || {
        heartbeat_ready.send(()).unwrap();
        heartbeat_db.heartbeat_agent_task_attempt(
            &heartbeat_owner,
            &heartbeat_task_id,
            &heartbeat_attempt_id,
            &heartbeat_assignee,
            &heartbeat_fence,
            1,
        )
    });

    let completion_db = Arc::clone(&db);
    let completion_owner = owner.clone();
    let completion_assignee = assignee.clone();
    let completion_task_id = completion_task.clone();
    let completion_attempt_id = completion_attempt.attempt.attempt_id.clone();
    let completion_fence = completion_attempt.attempt_fence.clone();
    let completion_ready = ready_tx.clone();
    let completion_handle = std::thread::spawn(move || {
        completion_ready.send(()).unwrap();
        completion_db.complete_agent_task_attempt(
            &completion_owner,
            &completion_task_id,
            &completion_attempt_id,
            &completion_assignee,
            &completion_fence,
            1,
            AgentTaskState::Succeeded,
            Some("late result"),
            None,
            "completion-clock-finish",
        )
    });

    ready_rx.recv().unwrap();
    ready_rx.recv().unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let expires_at = wall_now_ms().saturating_add(200);
    guard
        .execute(
            "UPDATE wc_agent_task_attempts
             SET lease_expires_at_unix_ms = ?1
             WHERE attempt_id IN (?2, ?3)",
            params![
                expires_at,
                heartbeat_attempt.attempt.attempt_id,
                completion_attempt.attempt.attempt_id,
            ],
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    drop(guard);

    let heartbeat_error = heartbeat_handle.join().unwrap().unwrap_err();
    assert_eq!(heartbeat_error.code(), "agent_task_attempt_stale");
    let completion_error = completion_handle.join().unwrap().unwrap_err();
    assert_eq!(completion_error.code(), "agent_task_attempt_stale");
}

#[test]
fn exact_start_retry_returns_same_attempt_and_fence_even_after_later_attempt() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("agent-task-start-replay.db")).unwrap();
    let owner = principal('5');
    let assignee = agent(&db, &owner, "retry-agent");
    let task_id = create_assigned_task(&db, &owner, &assignee, "retry-task");
    let first = start(&db, &owner, &task_id, &assignee, "start-one", T0 + 10);
    assert!(first
        .attempt
        .attempt_id
        .starts_with(AGENT_TASK_ATTEMPT_ID_PREFIX));
    assert!(first
        .attempt_fence
        .starts_with(AGENT_TASK_ATTEMPT_FENCE_PREFIX));
    let direct_replay = start(&db, &owner, &task_id, &assignee, "start-one", T0 + 11);
    assert_eq!(direct_replay.attempt.attempt_id, first.attempt.attempt_id);
    assert_eq!(direct_replay.attempt_fence, first.attempt_fence);
    assert!(direct_replay.replayed);

    let second = start(
        &db,
        &owner,
        &task_id,
        &assignee,
        "start-two",
        first.attempt.lease_expires_at_unix_ms,
    );
    assert_eq!(second.attempt.attempt_number, 2);
    assert_ne!(second.attempt.attempt_id, first.attempt.attempt_id);
    assert_ne!(second.attempt_fence, first.attempt_fence);

    let old_replay = start(
        &db,
        &owner,
        &task_id,
        &assignee,
        "start-one",
        second.attempt.started_at_unix_ms + 1,
    );
    assert_eq!(old_replay.attempt.attempt_id, first.attempt.attempt_id);
    assert_eq!(old_replay.attempt_fence, first.attempt_fence);
    assert_eq!(old_replay.attempt.state, AgentTaskAttemptState::Expired);
    assert_eq!(
        old_replay.task.latest_attempt.unwrap().attempt_id,
        second.attempt.attempt_id
    );
}

#[test]
fn lease_expiry_permanently_fences_old_attempt_and_never_transfers_assignment() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("agent-task-lease.db")).unwrap();
    let owner = principal('6');
    let agent_a = agent(&db, &owner, "lease-a");
    let agent_b = agent(&db, &owner, "lease-b");
    let task_id = create_assigned_task(&db, &owner, &agent_a, "lease-task");
    let first = start(&db, &owner, &task_id, &agent_a, "lease-start-1", T0 + 20);
    let before_expiry = first.attempt.lease_expires_at_unix_ms - 1;
    let heartbeat = db
        .heartbeat_agent_task_attempt_at(
            &owner,
            &task_id,
            &first.attempt.attempt_id,
            &agent_a,
            &first.attempt_fence,
            first.attempt.attempt_controller_generation,
            before_expiry,
        )
        .unwrap();
    assert!(heartbeat.attempt.lease_active);
    let extended_expiry = heartbeat.attempt.lease_expires_at_unix_ms;
    assert!(extended_expiry > first.attempt.lease_expires_at_unix_ms);

    let stale_heartbeat = db
        .heartbeat_agent_task_attempt_at(
            &owner,
            &task_id,
            &first.attempt.attempt_id,
            &agent_a,
            &first.attempt_fence,
            first.attempt.attempt_controller_generation,
            extended_expiry,
        )
        .unwrap_err();
    assert_eq!(stale_heartbeat.code(), "agent_task_attempt_stale");
    let stale_completion = db
        .complete_agent_task_attempt_at(
            &owner,
            &task_id,
            &first.attempt.attempt_id,
            &agent_a,
            &first.attempt_fence,
            first.attempt.attempt_controller_generation,
            AgentTaskState::Succeeded,
            Some("late"),
            None,
            "late-completion",
            extended_expiry,
        )
        .unwrap_err();
    assert_eq!(stale_completion.code(), "agent_task_attempt_stale");

    let wrong_agent = db
        .start_agent_task_attempt_at(
            &owner,
            &task_id,
            &agent_b,
            "wrong-agent-start",
            extended_expiry + 1,
        )
        .unwrap_err();
    assert_eq!(wrong_agent.code(), "agent_task_assignee_mismatch");

    let second = start(
        &db,
        &owner,
        &task_id,
        &agent_a,
        "lease-start-2",
        extended_expiry + 2,
    );
    assert_eq!(second.attempt.attempt_number, 2);
    assert_ne!(second.attempt_fence, first.attempt_fence);

    let permanently_stale = db
        .heartbeat_agent_task_attempt_at(
            &owner,
            &task_id,
            &first.attempt.attempt_id,
            &agent_a,
            &first.attempt_fence,
            first.attempt.attempt_controller_generation,
            extended_expiry + 3,
        )
        .unwrap_err();
    assert_eq!(permanently_stale.code(), "agent_task_attempt_stale");
}

#[test]
fn explicit_reassignment_after_expiry_changes_only_durable_assignee() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("agent-task-reassign.db")).unwrap();
    let owner = principal('7');
    let agent_a = agent(&db, &owner, "reassign-a");
    let agent_b = agent(&db, &owner, "reassign-b");
    let task_id = create_assigned_task(&db, &owner, &agent_a, "reassign-task");
    let first = start(&db, &owner, &task_id, &agent_a, "reassign-start-1", T0 + 30);

    let active_reassign = db
        .assign_agent_task_at(
            &owner,
            &task_id,
            &agent_b,
            first.attempt.lease_expires_at_unix_ms - 1,
        )
        .unwrap_err();
    assert_eq!(active_reassign.code(), "agent_task_attempt_active");

    let reassigned = db
        .assign_agent_task_at(
            &owner,
            &task_id,
            &agent_b,
            first.attempt.lease_expires_at_unix_ms,
        )
        .unwrap();
    assert!(reassigned.state_changed);
    assert_eq!(
        reassigned.task.summary.assignee_agent_id.as_deref(),
        Some(agent_b.as_str())
    );
    let second = start(
        &db,
        &owner,
        &task_id,
        &agent_b,
        "reassign-start-2",
        first.attempt.lease_expires_at_unix_ms + 1,
    );
    assert_eq!(second.attempt.assignee_agent_id, agent_b);
    assert_eq!(second.attempt.attempt_number, 2);
}

#[test]
fn controller_generation_fences_replaced_carrier_without_creating_attempt() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("agent-task-controller.db")).unwrap();
    let owner = principal('8');
    let assignee = agent(&db, &owner, "controller-agent");
    let task_id = create_assigned_task(&db, &owner, &assignee, "controller-task");
    let started = start(
        &db,
        &owner,
        &task_id,
        &assignee,
        "controller-start",
        T0 + 40,
    );
    assert_eq!(started.attempt.attempt_controller_generation, 1);

    let replaced = db
        .replace_agent_task_attempt_controller_at(
            &owner,
            &task_id,
            &started.attempt.attempt_id,
            &assignee,
            &started.attempt_fence,
            1,
            T0 + 41,
        )
        .unwrap();
    assert_eq!(replaced.attempt_id, started.attempt.attempt_id);
    assert_eq!(replaced.attempt_number, 1);
    assert_eq!(replaced.attempt_controller_generation, 2);

    let stale_generation = db
        .heartbeat_agent_task_attempt_at(
            &owner,
            &task_id,
            &started.attempt.attempt_id,
            &assignee,
            &started.attempt_fence,
            1,
            T0 + 42,
        )
        .unwrap_err();
    assert_eq!(stale_generation.code(), "agent_task_attempt_stale");
    let stale_completion = db
        .complete_agent_task_attempt_at(
            &owner,
            &task_id,
            &started.attempt.attempt_id,
            &assignee,
            &started.attempt_fence,
            1,
            AgentTaskState::Succeeded,
            Some("stale controller must not complete"),
            None,
            "stale-controller-completion",
            T0 + 42,
        )
        .unwrap_err();
    assert_eq!(stale_completion.code(), "agent_task_attempt_stale");
    db.heartbeat_agent_task_attempt_at(
        &owner,
        &task_id,
        &started.attempt.attempt_id,
        &assignee,
        &started.attempt_fence,
        2,
        T0 + 42,
    )
    .unwrap();

    let count: i64 = db
        .conn_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM wc_agent_task_attempts WHERE task_id = ?1",
            [task_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn completion_is_fenced_terminal_and_exactly_replayable() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("agent-task-completion.db")).unwrap();
    let owner = principal('9');
    let assignee = agent(&db, &owner, "completion-agent");
    let task_id = create_assigned_task(&db, &owner, &assignee, "completion-task");
    let started = start(
        &db,
        &owner,
        &task_id,
        &assignee,
        "completion-start",
        T0 + 50,
    );

    let completed = db
        .complete_agent_task_attempt_at(
            &owner,
            &task_id,
            &started.attempt.attempt_id,
            &assignee,
            &started.attempt_fence,
            1,
            AgentTaskState::Succeeded,
            Some("done"),
            Some("bounded result"),
            "completion-key",
            T0 + 51,
        )
        .unwrap();
    assert_eq!(completed.task.state, AgentTaskState::Succeeded);
    assert_eq!(completed.attempt.state, AgentTaskAttemptState::Succeeded);
    assert_eq!(completed.attempt.terminal_result.as_deref(), Some("done"));
    assert!(completed.state_changed);

    let replay = db
        .complete_agent_task_attempt_at(
            &owner,
            &task_id,
            &started.attempt.attempt_id,
            &assignee,
            &started.attempt_fence,
            1,
            AgentTaskState::Succeeded,
            Some("done"),
            Some("bounded result"),
            "completion-key",
            T0 + 99,
        )
        .unwrap();
    assert!(replay.replayed);
    assert!(!replay.state_changed);
    assert_eq!(replay.attempt.attempt_id, completed.attempt.attempt_id);

    let changed = db
        .complete_agent_task_attempt_at(
            &owner,
            &task_id,
            &started.attempt.attempt_id,
            &assignee,
            &started.attempt_fence,
            1,
            AgentTaskState::Succeeded,
            Some("changed"),
            Some("bounded result"),
            "completion-key",
            T0 + 100,
        )
        .unwrap_err();
    assert_eq!(changed.code(), "communication_idempotency_conflict");

    let terminal_start = db
        .start_agent_task_attempt_at(&owner, &task_id, &assignee, "after-terminal", T0 + 101)
        .unwrap_err();
    assert_eq!(terminal_start.code(), "agent_task_terminal");
}

#[test]
fn restart_preserves_attempt_number_fence_generation_and_replay() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("agent-task-restart.db");
    let owner = principal('a');
    let (task_id, assignee, first_fence, second_attempt_id, second_fence) = {
        let db = Database::open(&path).unwrap();
        let assignee = agent(&db, &owner, "restart-agent");
        let task_id = create_assigned_task(&db, &owner, &assignee, "restart-task");
        let first = start(&db, &owner, &task_id, &assignee, "restart-start-1", T0 + 60);
        let second = start(
            &db,
            &owner,
            &task_id,
            &assignee,
            "restart-start-2",
            first.attempt.lease_expires_at_unix_ms,
        );
        let replaced = db
            .replace_agent_task_attempt_controller_at(
                &owner,
                &task_id,
                &second.attempt.attempt_id,
                &assignee,
                &second.attempt_fence,
                1,
                second.attempt.started_at_unix_ms + 1,
            )
            .unwrap();
        assert_eq!(replaced.attempt_controller_generation, 2);
        (
            task_id,
            assignee,
            first.attempt_fence,
            second.attempt.attempt_id,
            second.attempt_fence,
        )
    };

    let reopened = Database::open(&path).unwrap();
    let task = reopened.read_agent_task(&owner, &task_id).unwrap();
    let latest = task.summary.latest_attempt.unwrap();
    assert_eq!(latest.attempt_id, second_attempt_id);
    assert_eq!(latest.attempt_number, 2);
    assert_eq!(latest.attempt_controller_generation, 2);

    let replay = reopened
        .start_agent_task_attempt_at(&owner, &task_id, &assignee, "restart-start-2", T0 + 61)
        .unwrap();
    assert_eq!(replay.attempt.attempt_id, second_attempt_id);
    assert_eq!(replay.attempt_fence, second_fence);
    assert_eq!(replay.attempt.attempt_controller_generation, 2);
    assert_ne!(replay.attempt_fence, first_fence);
}

#[test]
fn restart_preserves_terminal_replay_and_never_revives_expired_attempts() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("agent-task-restart-terminal.db");
    let owner = principal('1');
    let (
        assignee,
        completed_task_id,
        completed_attempt_id,
        completed_fence,
        expiring_task_id,
        expiring_attempt_id,
        expiring_fence,
        expiring_lease,
    ) = {
        let db = Database::open(&path).unwrap();
        let assignee = agent(&db, &owner, "restart-terminal-agent");

        let completed_task_id =
            create_assigned_task(&db, &owner, &assignee, "restart-terminal-task");
        let completed = start(
            &db,
            &owner,
            &completed_task_id,
            &assignee,
            "restart-terminal-start",
            T0 + 70,
        );
        db.complete_agent_task_attempt_at(
            &owner,
            &completed_task_id,
            &completed.attempt.attempt_id,
            &assignee,
            &completed.attempt_fence,
            1,
            AgentTaskState::Succeeded,
            Some("persisted terminal result"),
            Some("persisted terminal reason"),
            "restart-terminal-completion",
            T0 + 71,
        )
        .unwrap();

        let expiring_task_id =
            create_assigned_task(&db, &owner, &assignee, "restart-expiring-task");
        let expiring = start(
            &db,
            &owner,
            &expiring_task_id,
            &assignee,
            "restart-expiring-start",
            T0 + 80,
        );
        (
            assignee,
            completed_task_id,
            completed.attempt.attempt_id,
            completed.attempt_fence,
            expiring_task_id,
            expiring.attempt.attempt_id,
            expiring.attempt_fence,
            expiring.attempt.lease_expires_at_unix_ms,
        )
    };

    let reopened = Database::open(&path).unwrap();
    let completed = reopened
        .read_agent_task_at(&owner, &completed_task_id, T0 + 100)
        .unwrap();
    let completed_attempt = completed.summary.latest_attempt.unwrap();
    assert_eq!(completed.summary.state, AgentTaskState::Succeeded);
    assert_eq!(completed_attempt.attempt_id, completed_attempt_id);
    assert_eq!(
        completed_attempt.terminal_result.as_deref(),
        Some("persisted terminal result")
    );
    assert_eq!(
        completed_attempt.terminal_reason.as_deref(),
        Some("persisted terminal reason")
    );

    let completion_replay = reopened
        .complete_agent_task_attempt_at(
            &owner,
            &completed_task_id,
            &completed_attempt_id,
            &assignee,
            &completed_fence,
            1,
            AgentTaskState::Succeeded,
            Some("persisted terminal result"),
            Some("persisted terminal reason"),
            "restart-terminal-completion",
            T0 + 101,
        )
        .unwrap();
    assert!(completion_replay.replayed);
    assert!(!completion_replay.state_changed);
    assert_eq!(
        completion_replay.attempt.terminal_result.as_deref(),
        Some("persisted terminal result")
    );

    let expired = reopened
        .read_agent_task_at(&owner, &expiring_task_id, expiring_lease)
        .unwrap();
    assert_eq!(expired.summary.state, AgentTaskState::Ready);
    let expired_attempt = expired.summary.latest_attempt.unwrap();
    assert_eq!(expired_attempt.attempt_id, expiring_attempt_id);
    assert_eq!(expired_attempt.state, AgentTaskAttemptState::Expired);
    assert!(!expired_attempt.lease_active);

    let stale_after_restart = reopened
        .heartbeat_agent_task_attempt_at(
            &owner,
            &expiring_task_id,
            &expiring_attempt_id,
            &assignee,
            &expiring_fence,
            1,
            expiring_lease,
        )
        .unwrap_err();
    assert_eq!(stale_after_restart.code(), "agent_task_attempt_stale");
    let start_replay = reopened
        .start_agent_task_attempt_at(
            &owner,
            &expiring_task_id,
            &assignee,
            "restart-expiring-start",
            expiring_lease + 1,
        )
        .unwrap();
    assert!(start_replay.replayed);
    assert_eq!(start_replay.attempt.attempt_id, expiring_attempt_id);
    assert_eq!(start_replay.attempt_fence, expiring_fence);
    assert_eq!(start_replay.attempt.state, AgentTaskAttemptState::Expired);
}

#[test]
fn endpoint_detach_never_deletes_or_controls_agent_task_attempt_truth() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("agent-task-endpoint.db")).unwrap();
    let owner = principal('b');
    let assignee = agent(&db, &owner, "endpoint-independent-agent");
    let endpoint = db
        .attach_agent_endpoint(
            &owner,
            NewAgentEndpoint {
                agent_id: assignee.clone(),
                host: "test-host".to_string(),
                client_attachment_id: Some("browser-window".to_string()),
                wake_capable: false,
                idempotency_key: "task-endpoint".to_string(),
            },
        )
        .unwrap()
        .endpoint;
    let task_id = create_assigned_task(&db, &owner, &assignee, "endpoint-task");
    let started = start(&db, &owner, &task_id, &assignee, "endpoint-start", T0 + 70);
    db.detach_agent_endpoint(&owner, &endpoint.endpoint_id)
        .unwrap();

    let task = db.read_agent_task(&owner, &task_id).unwrap();
    let attempt = task.summary.latest_attempt.unwrap();
    assert_eq!(attempt.attempt_id, started.attempt.attempt_id);
    assert_eq!(attempt.attempt_number, 1);
    assert_eq!(attempt.assignee_agent_id, assignee);
}

#[test]
fn source_conversation_is_correlation_only_and_foreign_exact_ids_are_existence_hidden() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("agent-task-privacy.db")).unwrap();
    let owner = principal('c');
    let foreign = principal('d');
    let assignee = agent(&db, &owner, "privacy-agent");
    let conversation = db
        .create_conversation(
            &owner,
            NewConversation {
                title: Some("source".to_string()),
                agent_ids: vec![assignee.clone()],
                idempotency_key: "source-conversation".to_string(),
            },
        )
        .unwrap();
    let conversation_id = conversation.conversation.conversation.conversation_id;
    let message = db
        .post_conversation_message(
            &owner,
            NewConversationMessage {
                conversation_id: conversation_id.clone(),
                body: "Explicit source message, not an implicit Task.".to_string(),
                author_agent_id: None,
                endpoint_id: None,
                expected_controller_generation: None,
                recipient_agent_ids: Some(Vec::new()),
                reply_to: None,
                idempotency_key: Some("source-message".to_string()),
                wake_reply_id: None,
                reply_operation_index: None,
            },
        )
        .unwrap();
    let message_id = message.message.message_id;
    let mut input = task_input(Some(assignee.clone()), "source-task");
    input.source_conversation_id = Some(conversation_id.clone());
    input.source_message_id = Some(message_id.clone());
    let created = db.create_agent_task_at(&owner, input, T0 + 80).unwrap();
    let task_id = created.task.summary.task_id.clone();
    assert_eq!(
        created.task.summary.source_conversation_id.as_deref(),
        Some(conversation_id.as_str())
    );
    assert_eq!(
        created.task.summary.source_message_id.as_deref(),
        Some(message_id.as_str())
    );
    assert_eq!(
        created.task.instruction,
        "Perform bounded durable work without assuming a window or Endpoint."
    );
    assert_ne!(
        created.task.instruction,
        "Explicit source message, not an implicit Task."
    );

    let owner_page = db.list_agent_tasks(&owner, None, 0, 10).unwrap();
    assert_eq!(owner_page.total_count, 1);
    assert_eq!(owner_page.tasks.len(), 1);
    let foreign_page = db.list_agent_tasks(&foreign, None, 0, 10).unwrap();
    assert_eq!(foreign_page.total_count, 0);
    assert!(foreign_page.tasks.is_empty());

    // Give the foreign principal Conversation participation directly. This is
    // test-only setup proving that communication membership does not become
    // AgentTask authority.
    db.conn_for_tests()
        .execute(
            "INSERT INTO wc_conversation_participants (
                participant_id, conversation_id, participant_kind, agent_id,
                principal_kind, principal_digest, joined_at_unix_ms
             ) VALUES (?1, ?2, 'human', NULL, ?3, ?4, ?5)",
            params![
                format!("wc_participant_{}", "e".repeat(32)),
                conversation_id,
                foreign.kind,
                foreign.digest,
                T0 + 81,
            ],
        )
        .unwrap();
    let foreign_error = db.read_agent_task(&foreign, &task_id).unwrap_err();
    let missing_id = format!("{AGENT_TASK_ID_PREFIX}{}", "f".repeat(32));
    let missing_error = db.read_agent_task(&foreign, &missing_id).unwrap_err();
    assert_eq!(foreign_error.code(), "agent_task_not_found");
    assert_eq!(foreign_error.code(), missing_error.code());
    assert_eq!(foreign_error.message(), missing_error.message());

    let started = start(&db, &owner, &task_id, &assignee, "privacy-start", T0 + 82);
    let foreign_attempt = db
        .heartbeat_agent_task_attempt_at(
            &foreign,
            &task_id,
            &started.attempt.attempt_id,
            &assignee,
            &started.attempt_fence,
            1,
            T0 + 83,
        )
        .unwrap_err();
    assert_eq!(foreign_attempt.code(), "agent_task_not_found");
}

#[test]
fn random_fence_and_stale_controller_generation_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("agent-task-fence.db")).unwrap();
    let owner = principal('e');
    let assignee = agent(&db, &owner, "fence-agent");
    let task_id = create_assigned_task(&db, &owner, &assignee, "fence-task");
    let started = start(&db, &owner, &task_id, &assignee, "fence-start", T0 + 90);
    let random_fence = format!("{AGENT_TASK_ATTEMPT_FENCE_PREFIX}{}", "0".repeat(32));
    let fence_error = db
        .heartbeat_agent_task_attempt_at(
            &owner,
            &task_id,
            &started.attempt.attempt_id,
            &assignee,
            &random_fence,
            1,
            T0 + 91,
        )
        .unwrap_err();
    assert_eq!(fence_error.code(), "agent_task_attempt_stale");

    let generation_error = db
        .heartbeat_agent_task_attempt_at(
            &owner,
            &task_id,
            &started.attempt.attempt_id,
            &assignee,
            &started.attempt_fence,
            2,
            T0 + 91,
        )
        .unwrap_err();
    assert_eq!(generation_error.code(), "agent_task_attempt_stale");
}

#[test]
fn replay_record_and_effect_commit_atomically_for_create_start_and_completion() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("agent-task-atomicity.db")).unwrap();
    let owner = principal('f');
    let assignee = agent(&db, &owner, "atomic-agent");

    db.conn_for_tests()
        .execute_batch(
            "CREATE TRIGGER fail_task_create_replay
             BEFORE INSERT ON wc_communication_idempotency
             WHEN NEW.operation = 'create_agent_task'
             BEGIN SELECT RAISE(ABORT, 'forced task create replay failure'); END;",
        )
        .unwrap();
    let failed_create = db
        .create_agent_task_at(
            &owner,
            task_input(Some(assignee.clone()), "atomic-create"),
            T0 + 100,
        )
        .unwrap_err();
    assert_eq!(failed_create.code(), "communication_store_unavailable");
    assert_eq!(
        db.conn_for_tests()
            .query_row("SELECT COUNT(*) FROM wc_agent_tasks", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    db.conn_for_tests()
        .execute_batch("DROP TRIGGER fail_task_create_replay;")
        .unwrap();

    let task_id = create_assigned_task(&db, &owner, &assignee, "atomic-task");
    db.conn_for_tests()
        .execute_batch(
            "CREATE TRIGGER fail_task_start_replay
             BEFORE INSERT ON wc_communication_idempotency
             WHEN NEW.operation = 'start_agent_task_attempt'
             BEGIN SELECT RAISE(ABORT, 'forced task start replay failure'); END;",
        )
        .unwrap();
    let failed_start = db
        .start_agent_task_attempt_at(&owner, &task_id, &assignee, "atomic-start", T0 + 101)
        .unwrap_err();
    assert_eq!(failed_start.code(), "communication_store_unavailable");
    assert_eq!(
        db.conn_for_tests()
            .query_row(
                "SELECT COUNT(*) FROM wc_agent_task_attempts WHERE task_id = ?1",
                [task_id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    assert_eq!(
        db.read_agent_task(&owner, &task_id).unwrap().summary.state,
        AgentTaskState::Ready
    );
    db.conn_for_tests()
        .execute_batch("DROP TRIGGER fail_task_start_replay;")
        .unwrap();

    let started = start(&db, &owner, &task_id, &assignee, "atomic-start", T0 + 102);
    db.conn_for_tests()
        .execute_batch(
            "CREATE TRIGGER fail_task_completion_replay
             BEFORE INSERT ON wc_communication_idempotency
             WHEN NEW.operation = 'complete_agent_task_attempt'
             BEGIN SELECT RAISE(ABORT, 'forced task completion replay failure'); END;",
        )
        .unwrap();
    let failed_completion = db
        .complete_agent_task_attempt_at(
            &owner,
            &task_id,
            &started.attempt.attempt_id,
            &assignee,
            &started.attempt_fence,
            1,
            AgentTaskState::Succeeded,
            Some("would have completed"),
            None,
            "atomic-completion",
            T0 + 103,
        )
        .unwrap_err();
    assert_eq!(failed_completion.code(), "communication_store_unavailable");
    let still_active = db.read_agent_task_at(&owner, &task_id, T0 + 103).unwrap();
    assert_eq!(still_active.summary.state, AgentTaskState::Active);
    assert_eq!(
        still_active.summary.latest_attempt.unwrap().state,
        AgentTaskAttemptState::Active
    );
    db.conn_for_tests()
        .execute_batch("DROP TRIGGER fail_task_completion_replay;")
        .unwrap();
    db.complete_agent_task_attempt_at(
        &owner,
        &task_id,
        &started.attempt.attempt_id,
        &assignee,
        &started.attempt_fence,
        1,
        AgentTaskState::Succeeded,
        Some("committed"),
        None,
        "atomic-completion",
        T0 + 104,
    )
    .unwrap();
}

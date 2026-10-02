use super::agent_task::{NewAgentTask, DEFAULT_AGENT_TASK_ATTEMPT_LEASE_MS};
use super::communication::NewAgentIdentity;
use super::store_primitives::{CommunicationPrincipal, COMMUNICATION_PRINCIPAL_DIGEST_PREFIX};
use super::Database;

const T0: i64 = 1_000_000;

fn principal_with_kind(kind: &str, hex: char) -> CommunicationPrincipal {
    CommunicationPrincipal {
        kind: kind.to_string(),
        digest: format!(
            "{COMMUNICATION_PRINCIPAL_DIGEST_PREFIX}{}",
            hex.to_string().repeat(64)
        ),
    }
}

fn principal(hex: char) -> CommunicationPrincipal {
    principal_with_kind("user", hex)
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

fn assigned_task(
    db: &Database,
    owner: &CommunicationPrincipal,
    assignee: &str,
    key: &str,
) -> String {
    db.create_agent_task_at(
        owner,
        NewAgentTask {
            title: "Durable work".to_string(),
            instruction: "Perform bounded durable work without assuming a window or Endpoint."
                .to_string(),
            assignee_agent_id: Some(assignee.to_string()),
            source_conversation_id: None,
            source_message_id: None,
            referenced_project_id: None,
            idempotency_key: key.to_string(),
        },
        T0,
    )
    .unwrap()
    .task
    .summary
    .task_id
}

#[test]
fn agent_task_attempt_references_pin_generation_and_do_not_retarget() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("attempt-refs.db");
    let alice = principal('a');
    let bob = principal('b');
    let db = Database::open(&path).unwrap();
    let assignee = agent(&db, &alice, "alice-agent");
    let task_id = assigned_task(&db, &alice, &assignee, "alice-task");
    let started = db
        .start_agent_task_attempt_at(&alice, &task_id, &assignee, "attempt-1", T0)
        .unwrap();
    let fence = started.attempt_fence.clone();
    assert!(
        fence.starts_with("wc_agent_task_fence_"),
        "the selector must not replace the existing fence"
    );

    let first = db
        .get_or_create_agent_task_attempt_reference(
            &alice,
            &task_id,
            &started.attempt.attempt_id,
            &assignee,
            &fence,
            started.attempt.attempt_controller_generation,
            T0,
        )
        .unwrap();
    assert_eq!(first.ref_index, 1);
    assert_eq!(first.attempt_fence, fence);
    let replay = db
        .get_or_create_agent_task_attempt_reference(
            &alice,
            &task_id,
            &started.attempt.attempt_id,
            &assignee,
            &fence,
            1,
            T0 + 50,
        )
        .unwrap();
    assert_eq!(replay, first);
    let created_at: i64 = db
        .conn_for_tests()
        .query_row(
            "SELECT created_at_unix_ms FROM wc_agent_task_attempt_references
             WHERE principal_kind = ?1 AND principal_digest = ?2 AND ref_index = 1",
            [alice.kind.as_str(), alice.digest.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(created_at, T0, "reissuing a tuple must not rewrite the row");

    let next_generation = db
        .get_or_create_agent_task_attempt_reference(
            &alice,
            &task_id,
            &started.attempt.attempt_id,
            &assignee,
            &fence,
            2,
            T0 + 10,
        )
        .unwrap();
    assert_eq!(next_generation.ref_index, 2);
    assert_eq!(next_generation.attempt_controller_generation, 2);
    assert_eq!(
        db.lookup_agent_task_attempt_reference(&alice, 1)
            .unwrap()
            .unwrap(),
        first,
        "the old index stays pinned to generation 1"
    );

    let same_digest_other_kind = principal_with_kind("service", 'a');
    let other_kind_ref = db
        .get_or_create_agent_task_attempt_reference(
            &same_digest_other_kind,
            &task_id,
            &started.attempt.attempt_id,
            &assignee,
            &fence,
            3,
            T0 + 15,
        )
        .unwrap();
    assert_eq!(
        other_kind_ref.ref_index, 1,
        "principal kind participates in the selector namespace"
    );
    assert!(db
        .lookup_agent_task_attempt_reference(&same_digest_other_kind, 2)
        .unwrap()
        .is_none());

    let bob_same_tuple = db
        .get_or_create_agent_task_attempt_reference(
            &bob,
            &task_id,
            &started.attempt.attempt_id,
            &assignee,
            &fence,
            1,
            T0 + 20,
        )
        .unwrap();
    assert_eq!(bob_same_tuple.ref_index, 1);
    assert!(
        db.lookup_agent_task_attempt_reference(&bob, 2)
            .unwrap()
            .is_none(),
        "another principal must not see Alice's later index"
    );
    drop(db);

    let reopened = Database::open(&path).unwrap();
    assert_eq!(
        reopened
            .lookup_agent_task_attempt_reference(&alice, 2)
            .unwrap()
            .unwrap(),
        next_generation
    );
    assert!(reopened
        .get_or_create_agent_task_attempt_reference(
            &alice,
            &task_id,
            &started.attempt.attempt_id,
            &assignee,
            &fence,
            0,
            T0,
        )
        .is_err());
    assert!(reopened
        .get_or_create_agent_task_attempt_reference(
            &alice,
            "task",
            &started.attempt.attempt_id,
            &assignee,
            &fence,
            1,
            T0,
        )
        .is_err());
    assert!(reopened
        .get_or_create_agent_task_attempt_reference(
            &alice,
            &task_id,
            &started.attempt.attempt_id,
            &assignee,
            "wc_agent_task_fence_short",
            1,
            T0,
        )
        .is_err());
    assert!(reopened
        .lookup_agent_task_attempt_reference(&alice, 0)
        .is_err());
}

#[test]
fn agent_task_attempt_references_stay_pinned_across_expiry_takeover_and_controller_replacement() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("attempt-ref-lifecycle.db")).unwrap();
    let owner = principal('c');
    let assignee = agent(&db, &owner, "owner-agent");
    let successor = agent(&db, &owner, "successor-agent");
    let task_id = assigned_task(&db, &owner, &assignee, "lifecycle-task");
    let started = db
        .start_agent_task_attempt_at(&owner, &task_id, &assignee, "start-1", T0)
        .unwrap();
    let pinned = db
        .get_or_create_agent_task_attempt_reference(
            &owner,
            &task_id,
            &started.attempt.attempt_id,
            &assignee,
            &started.attempt_fence,
            started.attempt.attempt_controller_generation,
            T0,
        )
        .unwrap();
    let replayed = db
        .start_agent_task_attempt_at(&owner, &task_id, &assignee, "start-1", T0 + 1)
        .unwrap();
    assert!(replayed.replayed);
    assert_eq!(replayed.attempt_fence, started.attempt_fence);
    let replay_pin = db
        .get_or_create_agent_task_attempt_reference(
            &owner,
            &task_id,
            &replayed.attempt.attempt_id,
            &assignee,
            &replayed.attempt_fence,
            replayed.attempt.attempt_controller_generation,
            T0 + 1,
        )
        .unwrap();
    assert_eq!(replay_pin, pinned);

    let replaced = db
        .replace_agent_task_attempt_controller_at(
            &owner,
            &task_id,
            &started.attempt.attempt_id,
            &assignee,
            &started.attempt_fence,
            started.attempt.attempt_controller_generation,
            T0 + 2,
        )
        .unwrap();
    assert_eq!(
        replaced.attempt_controller_generation,
        started.attempt.attempt_controller_generation + 1
    );
    assert_eq!(
        db.lookup_agent_task_attempt_reference(&owner, pinned.ref_index)
            .unwrap()
            .unwrap()
            .attempt_controller_generation,
        started.attempt.attempt_controller_generation
    );
    let stale_generation = db.start_agent_task_endpoint_continuation_at(
        &owner,
        &task_id,
        &pinned.attempt_id,
        &pinned.assignee_agent_id,
        &pinned.attempt_fence,
        pinned.attempt_controller_generation,
        T0 + 3,
    );
    assert_eq!(
        stale_generation.unwrap_err().code(),
        "agent_task_attempt_stale"
    );
    db.start_agent_task_endpoint_continuation_at(
        &owner,
        &task_id,
        &started.attempt.attempt_id,
        &assignee,
        &started.attempt_fence,
        replaced.attempt_controller_generation,
        T0 + 3,
    )
    .unwrap();
    let successor_generation = db
        .get_or_create_agent_task_attempt_reference(
            &owner,
            &task_id,
            &started.attempt.attempt_id,
            &assignee,
            &started.attempt_fence,
            replaced.attempt_controller_generation,
            T0 + 4,
        )
        .unwrap();
    assert_ne!(successor_generation.ref_index, pinned.ref_index);
    assert_eq!(
        db.lookup_agent_task_attempt_reference(&owner, pinned.ref_index)
            .unwrap()
            .unwrap(),
        pinned
    );

    let other_task = assigned_task(&db, &owner, &assignee, "expiry-task");
    let expiring = db
        .start_agent_task_attempt_at(&owner, &other_task, &assignee, "start-expire", T0)
        .unwrap();
    let expired_pin = db
        .get_or_create_agent_task_attempt_reference(
            &owner,
            &other_task,
            &expiring.attempt.attempt_id,
            &assignee,
            &expiring.attempt_fence,
            expiring.attempt.attempt_controller_generation,
            T0,
        )
        .unwrap();
    let after_lease = T0 + DEFAULT_AGENT_TASK_ATTEMPT_LEASE_MS + 1;
    let expired = db.start_agent_task_endpoint_continuation_at(
        &owner,
        &other_task,
        &expired_pin.attempt_id,
        &expired_pin.assignee_agent_id,
        &expired_pin.attempt_fence,
        expired_pin.attempt_controller_generation,
        after_lease,
    );
    assert_eq!(expired.unwrap_err().code(), "agent_task_attempt_stale");
    db.assign_agent_task_at(&owner, &other_task, &successor, after_lease)
        .unwrap();
    let taken_over = db
        .start_agent_task_attempt_at(&owner, &other_task, &successor, "start-2", after_lease)
        .unwrap();
    assert_ne!(taken_over.attempt.attempt_id, expiring.attempt.attempt_id);
    assert_ne!(taken_over.attempt_fence, expiring.attempt_fence);
    let taken_over_pin = db
        .get_or_create_agent_task_attempt_reference(
            &owner,
            &other_task,
            &taken_over.attempt.attempt_id,
            &successor,
            &taken_over.attempt_fence,
            taken_over.attempt.attempt_controller_generation,
            after_lease,
        )
        .unwrap();
    assert_ne!(taken_over_pin.ref_index, expired_pin.ref_index);
    assert_eq!(
        db.lookup_agent_task_attempt_reference(&owner, expired_pin.ref_index)
            .unwrap()
            .unwrap(),
        expired_pin
    );
    let stale_attempt = db.start_agent_task_endpoint_continuation_at(
        &owner,
        &other_task,
        &expired_pin.attempt_id,
        &expired_pin.assignee_agent_id,
        &expired_pin.attempt_fence,
        expired_pin.attempt_controller_generation,
        after_lease + 1,
    );
    assert_eq!(
        stale_attempt.unwrap_err().code(),
        "agent_task_attempt_stale"
    );
    db.start_agent_task_endpoint_continuation_at(
        &owner,
        &other_task,
        &taken_over.attempt.attempt_id,
        &successor,
        &taken_over.attempt_fence,
        taken_over.attempt.attempt_controller_generation,
        after_lease + 1,
    )
    .unwrap();
}

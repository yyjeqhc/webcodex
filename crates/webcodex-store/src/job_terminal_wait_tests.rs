use super::*;
use tempfile::tempdir;

const T0: i64 = 1_000;

fn principal(ch: char) -> JobTerminalWaitPrincipal {
    JobTerminalWaitPrincipal {
        kind: "test_principal".to_string(),
        digest: ch.to_string().repeat(64),
    }
}

fn source(job_id: &str) -> JobTerminalSourceIdentity {
    JobTerminalSourceIdentity {
        job_id: job_id.to_string(),
        client_id: "runner-a".to_string(),
        auth_kind: "managed_owner".to_string(),
        auth_value: Some("alice".to_string()),
    }
}

fn fact(job_id: &str, observed_at: i64) -> JobTerminalFact {
    JobTerminalFact {
        source: source(job_id),
        status: "completed".to_string(),
        outcome: "succeeded".to_string(),
        terminal_observed_at: observed_at,
        expires_at: observed_at + 900,
    }
}

fn waiting(job_id: &str, key: &str, expires_at: i64) -> NewJobTerminalWait {
    NewJobTerminalWait {
        source: source(job_id),
        idempotency_key: key.to_string(),
        expires_at,
        already_terminal: None,
    }
}

#[test]
fn terminal_fact_without_waits_does_not_require_a_sqlite_write_lock() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("job-terminal-read-fast-path.db");
    let db = Database::open(&path).unwrap();
    let external = rusqlite::Connection::open(&path).unwrap();
    external.execute_batch("BEGIN IMMEDIATE").unwrap();

    let matched = db
        .match_job_terminal_fact(&fact("job-without-wait", T0 + 1), T0 + 1)
        .unwrap();
    assert_eq!(matched.matched_count, 0);
    assert!(matched.delivery_candidates.is_empty());

    external.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn terminal_fact_fast_path_still_prunes_expired_waits() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("job-terminal-prune-fast-path.db")).unwrap();
    let owner = principal('d');
    db.create_job_terminal_wait(&owner, waiting("expired-job", "expired", T0 + 1), T0)
        .unwrap();

    let matched = db
        .match_job_terminal_fact(&fact("different-job", T0 + 2), T0 + 2)
        .unwrap();
    assert_eq!(matched.matched_count, 0);
    assert!(matched.delivery_candidates.is_empty());

    let count: i64 = db
        .conn_for_tests()
        .query_row("SELECT COUNT(*) FROM wc_job_terminal_waits", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn terminal_fact_accepts_legacy_timeout_status() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("job-terminal-timeout.db")).unwrap();
    let owner = principal('a');
    db.create_job_terminal_wait(
        &owner,
        waiting("timeout-job", "timeout-key", T0 + 8_100),
        T0,
    )
    .unwrap();
    let mut terminal = fact("timeout-job", T0 + 20);
    terminal.status = "timeout".to_string();
    terminal.outcome = "timed_out".to_string();

    let matched = db.match_job_terminal_fact(&terminal, T0 + 20).unwrap();
    assert_eq!(matched.matched_count, 1);
    let wait = db
        .read_job_terminal_wait(&owner, &matched.delivery_candidates[0].1, T0 + 21)
        .unwrap();
    assert_eq!(wait.terminal_status.as_deref(), Some("timeout"));
    assert_eq!(wait.terminal_outcome.as_deref(), Some("timed_out"));
}

#[test]
fn lost_terminal_fact_roundtrips_outcome_unknown() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("job-terminal-lost.db")).unwrap();
    let owner = principal('c');
    db.create_job_terminal_wait(&owner, waiting("lost-job", "lost-key", T0 + 8_100), T0)
        .unwrap();
    let mut terminal = fact("lost-job", T0 + 20);
    terminal.status = "lost".to_string();
    terminal.outcome = "outcome_unknown".to_string();

    let matched = db.match_job_terminal_fact(&terminal, T0 + 20).unwrap();
    assert_eq!(matched.matched_count, 1);
    let wait = db
        .read_job_terminal_wait(&owner, &matched.delivery_candidates[0].1, T0 + 21)
        .unwrap();
    assert_eq!(wait.terminal_status.as_deref(), Some("lost"));
    assert_eq!(wait.terminal_outcome.as_deref(), Some("outcome_unknown"));
}

#[test]
fn keyed_registration_matching_and_owner_partition_are_one_shot() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("job-terminal-wait.db")).unwrap();
    let owner = principal('a');
    let foreign = principal('b');

    let created = db
        .create_job_terminal_wait(&owner, waiting("job-a", "arm-a", T0 + 8_100), T0)
        .unwrap();
    assert_eq!(created.wait.state, JobTerminalWaitState::Waiting);
    assert_eq!(
        created.wait.delivery_state,
        JobTerminalDeliveryState::NotReady
    );
    assert!(!created.replayed);

    // Derived active deadlines move with wall time and are deliberately not part
    // of keyed request identity.
    let replay = db
        .create_job_terminal_wait(&owner, waiting("job-a", "arm-a", T0 + 8_200), T0 + 10)
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.wait.wait_id, created.wait.wait_id);

    let foreign_error = db
        .read_job_terminal_wait(&foreign, &created.wait.wait_id, T0 + 10)
        .unwrap_err();
    assert_eq!(foreign_error.code, "job_terminal_wait_not_found");

    let terminal = fact("job-a", T0 + 20);
    let first = db.match_job_terminal_fact(&terminal, T0 + 20).unwrap();
    assert_eq!(first.matched_count, 1);
    assert_eq!(first.delivery_candidates.len(), 1);
    let duplicate = db.match_job_terminal_fact(&terminal, T0 + 21).unwrap();
    assert_eq!(duplicate.matched_count, 0);

    let triggered = db
        .read_job_terminal_wait(&owner, &created.wait.wait_id, T0 + 21)
        .unwrap();
    assert_eq!(triggered.state, JobTerminalWaitState::Triggered);
    assert_eq!(triggered.delivery_state, JobTerminalDeliveryState::Pending);
    assert_eq!(triggered.terminal_status.as_deref(), Some("completed"));
    assert_eq!(triggered.terminal_outcome.as_deref(), Some("succeeded"));

    let immediate = db
        .create_job_terminal_wait(
            &owner,
            NewJobTerminalWait {
                source: source("job-a"),
                idempotency_key: "arm-after-terminal".to_string(),
                expires_at: terminal.expires_at,
                already_terminal: Some(terminal.clone()),
            },
            T0 + 22,
        )
        .unwrap();
    assert_eq!(immediate.wait.state, JobTerminalWaitState::Triggered);
    assert_eq!(
        immediate.wait.delivery_state,
        JobTerminalDeliveryState::Pending
    );

    let conflict = db
        .create_job_terminal_wait(&owner, waiting("job-b", "arm-a", T0 + 8_100), T0 + 23)
        .unwrap_err();
    assert_eq!(conflict.code, "job_terminal_wait_idempotency_conflict");
}

#[test]
fn repeated_terminal_fact_reoffers_only_safe_pending_deliveries() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("job-terminal-pending-retry.db")).unwrap();
    let first_owner = principal('e');
    let second_owner = principal('f');
    let terminal = fact("job-retry", T0 + 20);

    let first_wait = db
        .create_job_terminal_wait(
            &first_owner,
            waiting("job-retry", "retry-first", T0 + 8_100),
            T0,
        )
        .unwrap()
        .wait
        .wait_id;
    let second_wait = db
        .create_job_terminal_wait(
            &second_owner,
            waiting("job-retry", "retry-second", T0 + 8_100),
            T0,
        )
        .unwrap()
        .wait
        .wait_id;

    let first_match = db.match_job_terminal_fact(&terminal, T0 + 20).unwrap();
    assert_eq!(first_match.matched_count, 2);
    assert_eq!(first_match.delivery_candidates.len(), 2);

    // Replaying the same canonical terminal event after a post-match sink
    // failure must recover pending delivery work without pretending a new wait
    // matched the terminal transition.
    let retry = db.match_job_terminal_fact(&terminal, T0 + 21).unwrap();
    assert_eq!(retry.matched_count, 0);
    assert_eq!(retry.delivery_candidates.len(), 2);

    let prepared = db
        .prepare_job_terminal_delivery(&first_owner, &first_wait, T0 + 22)
        .unwrap()
        .unwrap();
    let after_prepare = db.match_job_terminal_fact(&terminal, T0 + 23).unwrap();
    assert_eq!(after_prepare.matched_count, 0);
    assert_eq!(after_prepare.delivery_candidates.len(), 1);
    assert_eq!(after_prepare.delivery_candidates[0].1, second_wait);

    db.finish_job_terminal_delivery(
        &first_owner,
        &first_wait,
        &prepared.attempt_id,
        true,
        T0 + 24,
    )
    .unwrap();
    let second_prepared = db
        .prepare_job_terminal_delivery(&second_owner, &second_wait, T0 + 25)
        .unwrap()
        .unwrap();
    db.finish_job_terminal_delivery(
        &second_owner,
        &second_wait,
        &second_prepared.attempt_id,
        false,
        T0 + 26,
    )
    .unwrap();

    let finished = db.match_job_terminal_fact(&terminal, T0 + 27).unwrap();
    assert_eq!(finished.matched_count, 0);
    assert!(finished.delivery_candidates.is_empty());
}

#[test]
fn waiting_rows_survive_reopen_and_prepared_delivery_recovers_unknown_without_retry() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("job-terminal-restart.db");
    let owner = principal('c');
    let wait_id = {
        let db = Database::open(&path).unwrap();
        let created = db
            .create_job_terminal_wait(
                &owner,
                waiting("job-restart", "arm-restart", T0 + 8_100),
                T0,
            )
            .unwrap();
        created.wait.wait_id
    };

    let db = Database::open(&path).unwrap();
    let restored = db.read_job_terminal_wait(&owner, &wait_id, T0 + 1).unwrap();
    assert_eq!(restored.state, JobTerminalWaitState::Waiting);
    db.match_job_terminal_fact(&fact("job-restart", T0 + 2), T0 + 2)
        .unwrap();
    let prepared = db
        .prepare_job_terminal_delivery(&owner, &wait_id, T0 + 3)
        .unwrap()
        .unwrap();
    assert_eq!(
        prepared.wait.delivery_state,
        JobTerminalDeliveryState::Prepared
    );
    drop(db);

    let reopened = Database::open(&path).unwrap();
    assert_eq!(
        reopened
            .recover_job_terminal_deliveries_after_restart(T0 + 4)
            .unwrap(),
        1
    );
    let unknown = reopened
        .read_job_terminal_wait(&owner, &wait_id, T0 + 4)
        .unwrap();
    assert_eq!(
        unknown.delivery_state,
        JobTerminalDeliveryState::DeliveryUnknown
    );
    assert!(reopened
        .prepare_job_terminal_delivery(&owner, &wait_id, T0 + 5)
        .unwrap()
        .is_none());

    let delivered = reopened
        .create_job_terminal_wait(
            &owner,
            NewJobTerminalWait {
                source: source("job-delivered"),
                idempotency_key: "arm-delivered".to_string(),
                expires_at: T0 + 950,
                already_terminal: Some(fact("job-delivered", T0 + 50)),
            },
            T0 + 50,
        )
        .unwrap();
    let prepared = reopened
        .prepare_job_terminal_delivery(&owner, &delivered.wait.wait_id, T0 + 51)
        .unwrap()
        .unwrap();
    let final_wait = reopened
        .finish_job_terminal_delivery(
            &owner,
            &delivered.wait.wait_id,
            &prepared.attempt_id,
            true,
            T0 + 52,
        )
        .unwrap();
    assert_eq!(
        final_wait.delivery_state,
        JobTerminalDeliveryState::Delivered
    );
    assert_eq!(
        reopened
            .finish_job_terminal_delivery(
                &owner,
                &delivered.wait.wait_id,
                &prepared.attempt_id,
                true,
                T0 + 53,
            )
            .unwrap_err()
            .code,
        "job_terminal_delivery_fence_mismatch"
    );
}

#[test]
fn expired_and_malformed_durable_rows_fail_closed() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("job-terminal-corrupt.db")).unwrap();
    let owner = principal('d');

    let expired = db
        .create_job_terminal_wait(&owner, waiting("job-expired", "arm-expired", T0 + 1), T0)
        .unwrap();
    assert_eq!(
        db.read_job_terminal_wait(&owner, &expired.wait.wait_id, T0 + 2)
            .unwrap_err()
            .code,
        "job_terminal_wait_not_found"
    );

    let terminal = fact("job-corrupt", T0 + 10);
    let created = db
        .create_job_terminal_wait(
            &owner,
            NewJobTerminalWait {
                source: source("job-corrupt"),
                idempotency_key: "arm-corrupt".to_string(),
                expires_at: terminal.expires_at,
                already_terminal: Some(terminal),
            },
            T0 + 10,
        )
        .unwrap();
    {
        let conn = db.lock_connection(crate::StoreDomain::JobTerminalWait);
        conn.execute(
            "UPDATE wc_job_terminal_waits SET terminal_status='not-a-job-status' WHERE wait_id=?1",
            [&created.wait.wait_id],
        )
        .unwrap();
    }
    assert_eq!(
        db.read_job_terminal_wait(&owner, &created.wait.wait_id, T0 + 11)
            .unwrap_err()
            .code,
        "job_terminal_wait_storage_invariant"
    );
}

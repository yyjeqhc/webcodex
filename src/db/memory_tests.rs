use super::*;
use crate::db::memory::{
    memory_definition_hash, memory_state_revision, MemoryPrincipalAttribution, MemoryPriority,
    MemoryScopeAttribution, MemorySetInput, MEMORY_SCOPE_IDENTITY_ATTRIBUTED,
};
use rusqlite::{params, Connection};
use std::sync::{Arc, Barrier};

fn scope(ch: char) -> String {
    format!("wc_memscope_{}", ch.to_string().repeat(64))
}

fn input(key: &str, summary: &str) -> MemorySetInput {
    MemorySetInput {
        memory_key: key.to_string(),
        summary: summary.to_string(),
        body: "Detailed project guidance.".to_string(),
        priority: MemoryPriority::Normal,
        bootstrap: false,
        tags: vec!["architecture".to_string(), "stable".to_string()],
        expected_revision: None,
    }
}

fn scope_attribution(project: &str, runner: &str, hex: char) -> MemoryScopeAttribution {
    MemoryScopeAttribution {
        project_runtime_id: project.to_string(),
        runner_client_id: runner.to_string(),
        root_fingerprint: format!("wc_memroot_{}", hex.to_string().repeat(64)),
    }
}

fn principal(kind: &str, hex: char) -> MemoryPrincipalAttribution {
    MemoryPrincipalAttribution {
        kind: kind.to_string(),
        principal_digest: format!("wc_memprincipal_{}", hex.to_string().repeat(64)),
    }
}

#[test]
fn memory_create_retry_cas_update_delete_and_restart_are_durable() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("memory.db");
    let db = Database::open(&path).unwrap();
    let scope = scope('a');

    let created = db
        .set_project_memory(&scope, input("deployment-policy", "Use staged deploys."))
        .unwrap();
    assert!(created.created && created.state_changed);
    assert!(created.record.memory_id.starts_with("wc_mem_"));
    assert!(created.record.revision.starts_with("wc_memrev_"));

    let retried = db
        .set_project_memory(&scope, input("deployment-policy", "Use staged deploys."))
        .unwrap();
    assert!(!retried.created && !retried.state_changed);
    assert_eq!(retried.record.memory_id, created.record.memory_id);
    assert_eq!(retried.record.revision, created.record.revision);

    let mut changed_without_cas = input("deployment-policy", "Use canary deploys.");
    assert_eq!(
        db.set_project_memory(&scope, changed_without_cas.clone())
            .unwrap_err()
            .code(),
        "memory_expected_revision_required"
    );

    changed_without_cas.expected_revision = Some(created.record.revision.clone());
    let updated = db.set_project_memory(&scope, changed_without_cas).unwrap();
    assert!(updated.state_changed && !updated.created);
    assert_eq!(
        updated.old_revision.as_deref(),
        Some(created.record.revision.as_str())
    );
    assert_ne!(updated.record.revision, created.record.revision);

    let mut stale = input("deployment-policy", "Overwrite stale.");
    stale.expected_revision = Some(created.record.revision.clone());
    assert_eq!(
        db.set_project_memory(&scope, stale).unwrap_err().code(),
        "memory_changed"
    );

    drop(db);
    let reopened = Database::open(&path).unwrap();
    let durable = reopened
        .get_project_memory(&scope, "deployment-policy")
        .unwrap()
        .unwrap();
    assert_eq!(durable.revision, updated.record.revision);
    assert_eq!(durable.summary, "Use canary deploys.");
    assert_eq!(durable.created_by_kind, "dev");
    assert_eq!(durable.updated_by_kind, "dev");
    assert!(durable.created_by_principal_digest.is_some());
    assert_eq!(
        durable.created_by_principal_digest,
        durable.updated_by_principal_digest
    );
    let durable_scope = reopened
        .get_project_memory_scope(&scope)
        .unwrap()
        .expect("attributed scope must survive reopen");
    assert_eq!(
        durable_scope.scope.identity_state,
        MEMORY_SCOPE_IDENTITY_ATTRIBUTED
    );
    assert_eq!(
        durable_scope.scope.project_runtime_id.as_deref(),
        Some("agent:test:memory")
    );
    assert_eq!(
        durable_scope.scope.runner_client_id.as_deref(),
        Some("test-runner")
    );
    assert_eq!(durable_scope.memories.len(), 1);

    assert_eq!(
        reopened
            .delete_project_memory(&scope, "deployment-policy", &created.record.revision)
            .unwrap_err()
            .code(),
        "memory_changed"
    );
    let deleted = reopened
        .delete_project_memory(&scope, "deployment-policy", &updated.record.revision)
        .unwrap();
    assert!(deleted.deleted && deleted.state_changed);
    let desired_state_retry = reopened
        .delete_project_memory(&scope, "deployment-policy", &updated.record.revision)
        .unwrap();
    assert!(!desired_state_retry.deleted && !desired_state_retry.state_changed);
}

#[test]
fn memory_scope_and_revision_identity_are_content_and_scope_safe() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("memory.db")).unwrap();
    let scope_a = scope('a');
    let scope_b = scope('b');
    let created = db
        .set_project_memory(&scope_a, input("architecture-decisions", "A-only memory"))
        .unwrap();
    assert!(db
        .get_project_memory(&scope_b, "architecture-decisions")
        .unwrap()
        .is_none());
    assert!(db.list_project_memories(&scope_b).unwrap().is_empty());
    assert!(!created.record.memory_id.contains(&scope_a));

    let tags_a = vec!["a".to_string(), "b".to_string()];
    let tags_b = vec!["b".to_string(), "a".to_string()];
    let canonical_b = canonicalize_memory_tags(tags_b).unwrap();
    assert_eq!(
        memory_definition_hash("k", "s", "b", MemoryPriority::High, true, &tags_a),
        memory_definition_hash("k", "s", "b", MemoryPriority::High, true, &canonical_b)
    );
    assert_ne!(
        memory_definition_hash("k", "s", "b", MemoryPriority::High, true, &tags_a),
        memory_definition_hash("k", "s", "changed", MemoryPriority::High, true, &tags_a)
    );
    assert!(created.record.definition_hash.starts_with("wc_memdef_"));
    assert_eq!(created.record.generation, 1);
}

#[test]
fn memory_bounds_and_project_capacity_fail_closed_without_blocking_existing_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("memory.db")).unwrap();
    let scope = scope('c');

    assert!(validate_memory_key(".").is_err());
    assert!(validate_memory_key("bad/key").is_err());
    assert!(validate_memory_summary(&"x".repeat(MAX_MEMORY_SUMMARY_CHARS + 1)).is_err());
    assert!(validate_memory_body(&"x".repeat(MAX_MEMORY_BODY_BYTES + 1)).is_err());
    assert!(validate_memory_query(&"x".repeat(MAX_MEMORY_QUERY_CHARS + 1)).is_err());
    assert!(canonicalize_memory_tags(vec!["x".repeat(MAX_MEMORY_TAG_CHARS + 1)]).is_err());
    assert!(
        canonicalize_memory_tags((0..=MAX_MEMORY_TAGS).map(|i| format!("t{i}")).collect()).is_err()
    );

    for index in 0..MAX_MEMORIES_PER_PROJECT {
        db.set_project_memory(&scope, input(&format!("k{index:03}"), "bounded"))
            .unwrap();
    }
    assert_eq!(
        db.list_project_memories(&scope).unwrap().len(),
        MAX_MEMORIES_PER_PROJECT
    );
    assert_eq!(
        db.set_project_memory(&scope, input("overflow", "no eviction"))
            .unwrap_err()
            .code(),
        "memory_project_capacity_exceeded"
    );

    let existing = db.get_project_memory(&scope, "k000").unwrap().unwrap();
    let mut update = input("k000", "updated at capacity");
    update.expected_revision = Some(existing.revision.clone());
    let updated = db.set_project_memory(&scope, update).unwrap();
    assert!(updated.state_changed);
    let deleted = db
        .delete_project_memory(
            &scope,
            "k001",
            &db.get_project_memory(&scope, "k001")
                .unwrap()
                .unwrap()
                .revision,
        )
        .unwrap();
    assert!(deleted.deleted);
}

#[test]
fn memory_global_capacity_is_hard_and_does_not_evict() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("memory.db")).unwrap();
    {
        let mut conn = db.conn_for_tests();
        let tx = conn.transaction().unwrap();
        for index in 0..MAX_MEMORIES_GLOBAL {
            let memory_id = format!("wc_mem_{index:032x}");
            let memory_scope_id = format!("wc_memscope_{:064x}", index + 100);
            let memory_key = format!("k{index}");
            let definition_hash =
                memory_definition_hash(&memory_key, "s", "", MemoryPriority::Normal, false, &[]);
            let revision = memory_state_revision(&memory_scope_id, &memory_id, 1, &definition_hash);
            tx.execute(
                "INSERT INTO project_memories
                 (memory_id, memory_scope_id, memory_key, summary, body, priority, bootstrap,
                  tags_json, definition_hash, generation, revision,
                  created_at_unix_ms, updated_at_unix_ms,
                  created_by_kind, created_by_principal_digest,
                  updated_by_kind, updated_by_principal_digest)
                 VALUES (?1, ?2, ?3, 's', '', 'normal', 0, '[]', ?4, 1, ?5, 1, 1,
                         'dev', ?6, 'dev', ?6)",
                params![
                    memory_id,
                    memory_scope_id,
                    memory_key,
                    definition_hash,
                    revision,
                    format!("wc_memprincipal_{}", "1".repeat(64))
                ],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO project_memory_scopes
                 (memory_scope_id, identity_state, project_runtime_id, runner_client_id,
                  root_fingerprint, created_at_unix_ms, last_mutated_at_unix_ms)
                 VALUES (?1, 'attributed', ?2, 'test-runner', ?3, 1, 1)",
                params![
                    memory_scope_id,
                    format!("agent:test:global-{index}"),
                    format!("wc_memroot_{:064x}", index + 1),
                ],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }
    let new_scope = scope('f');
    assert_eq!(
        db.set_project_memory(&new_scope, input("new", "global full"))
            .unwrap_err()
            .code(),
        "memory_global_capacity_exceeded"
    );
    let total: i64 = db
        .conn_for_tests()
        .query_row("SELECT COUNT(*) FROM project_memories", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(total as usize, MAX_MEMORIES_GLOBAL);

    let reclaim_scope = format!("wc_memscope_{:064x}", 100);
    let reclaim = db
        .get_project_memory_scope(&reclaim_scope)
        .unwrap()
        .unwrap();
    let reclaim_catalog = memory_catalog_revision(&reclaim.memories);
    let purged = db
        .purge_project_memory_scope(&reclaim_scope, &reclaim_catalog)
        .unwrap();
    assert!(purged.purged && purged.state_changed);
    assert_eq!(purged.purged_count, 1);
    assert!(
        db.set_project_memory(&new_scope, input("new", "capacity recovered"))
            .unwrap()
            .created
    );
    let recovered_total: i64 = db
        .conn_for_tests()
        .query_row("SELECT COUNT(*) FROM project_memories", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(recovered_total as usize, MAX_MEMORIES_GLOBAL);
    assert_eq!(total as usize, MAX_MEMORIES_GLOBAL);
}

#[test]
fn memory_transaction_failure_rolls_back_and_concurrent_cas_has_one_winner() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(Database::open(&tmp.path().join("memory.db")).unwrap());
    let scope = scope('d');
    let created = db
        .set_project_memory(&scope, input("test-strategy", "before"))
        .unwrap();

    {
        let conn = db.conn_for_tests();
        conn.execute_batch(
            "CREATE TRIGGER fail_memory_update BEFORE UPDATE ON project_memories
             BEGIN SELECT RAISE(ABORT, 'forced memory failure'); END;",
        )
        .unwrap();
    }
    let mut failed = input("test-strategy", "after");
    failed.expected_revision = Some(created.record.revision.clone());
    assert_eq!(
        db.set_project_memory(&scope, failed).unwrap_err().code(),
        "memory_store_unavailable"
    );
    assert_eq!(
        db.get_project_memory(&scope, "test-strategy")
            .unwrap()
            .unwrap()
            .summary,
        "before"
    );
    db.conn_for_tests()
        .execute_batch("DROP TRIGGER fail_memory_update;")
        .unwrap();

    let barrier = Arc::new(Barrier::new(3));
    let mut handles = Vec::new();
    for summary in ["writer-a", "writer-b"] {
        let db = db.clone();
        let scope = scope.clone();
        let revision = created.record.revision.clone();
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            let mut update = input("test-strategy", summary);
            update.expected_revision = Some(revision);
            barrier.wait();
            db.set_project_memory(&scope, update)
        }));
    }
    barrier.wait();
    let outcomes = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, Err(MemoryStoreError::Changed { .. })))
            .count(),
        1
    );
}

#[test]
fn stale_delete_retry_does_not_delete_identical_recreation() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("memory.db")).unwrap();
    let scope = scope('a');
    let created = db
        .set_project_memory(&scope, input("policy", "same definition"))
        .unwrap();
    let r1 = created.record.revision.clone();
    let m1 = created.record.memory_id.clone();
    db.delete_project_memory(&scope, "policy", &r1).unwrap();
    let recreated = db
        .set_project_memory(&scope, input("policy", "same definition"))
        .unwrap();
    assert_ne!(recreated.record.memory_id, m1);
    assert_ne!(recreated.record.revision, r1);
    assert_eq!(
        recreated.record.definition_hash,
        created.record.definition_hash
    );
    assert_eq!(
        db.delete_project_memory(&scope, "policy", &r1)
            .unwrap_err()
            .code(),
        "memory_changed"
    );
    assert_eq!(
        db.get_project_memory(&scope, "policy")
            .unwrap()
            .unwrap()
            .memory_id,
        recreated.record.memory_id
    );
}

#[test]
fn stale_revision_is_rejected_after_a_b_a_transition_and_catalog_fences_recreation() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("memory.db")).unwrap();
    let scope = scope('b');
    let a1 = db.set_project_memory(&scope, input("policy", "A")).unwrap();
    let r1 = a1.record.revision.clone();
    let catalog_before = memory_catalog_revision(&db.list_project_memories(&scope).unwrap());

    let mut to_b = input("policy", "B");
    to_b.expected_revision = Some(r1.clone());
    let b = db.set_project_memory(&scope, to_b).unwrap();
    let mut back_to_a = input("policy", "A");
    back_to_a.expected_revision = Some(b.record.revision.clone());
    let a3 = db.set_project_memory(&scope, back_to_a).unwrap();
    assert_eq!(a3.record.generation, 3);
    assert_ne!(a3.record.revision, r1);
    assert_eq!(a3.record.definition_hash, a1.record.definition_hash);

    let mut stale_update = input("policy", "stale overwrite");
    stale_update.expected_revision = Some(r1.clone());
    assert_eq!(
        db.set_project_memory(&scope, stale_update)
            .unwrap_err()
            .code(),
        "memory_changed"
    );
    assert_eq!(
        db.delete_project_memory(&scope, "policy", &r1)
            .unwrap_err()
            .code(),
        "memory_changed"
    );

    let catalog_before_recreate =
        memory_catalog_revision(&db.list_project_memories(&scope).unwrap());
    let current = a3.record.revision.clone();
    db.delete_project_memory(&scope, "policy", &current)
        .unwrap();
    let recreated = db.set_project_memory(&scope, input("policy", "A")).unwrap();
    let catalog_after = memory_catalog_revision(&db.list_project_memories(&scope).unwrap());
    assert_ne!(catalog_before, catalog_after);
    assert_ne!(catalog_before_recreate, catalog_after);
    assert_ne!(recreated.record.revision, r1);
}

#[test]
fn identical_definition_cas_update_does_not_advance_generation() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("memory.db")).unwrap();
    let scope = scope('c');
    let created = db
        .set_project_memory(&scope, input("policy", "same"))
        .unwrap();
    let mut identical = input("policy", "same");
    identical.expected_revision = Some(created.record.revision.clone());
    let no_op = db.set_project_memory(&scope, identical).unwrap();
    assert!(!no_op.state_changed);
    assert_eq!(no_op.record.generation, 1);
    assert_eq!(no_op.record.revision, created.record.revision);
    let desired_state = db
        .set_project_memory(&scope, input("policy", "same"))
        .unwrap();
    assert!(!desired_state.state_changed);
    assert_eq!(desired_state.record.generation, 1);
}

#[test]
fn corrupted_persisted_memory_rows_fail_closed_before_projection() {
    let cases: Vec<(&str, Box<dyn Fn(&Connection)>)> = vec![
        (
            "overlarge_body",
            Box::new(|conn| {
                conn.execute(
                    "UPDATE project_memories SET body = ?1",
                    params!["x".repeat(MAX_MEMORY_BODY_BYTES + 1)],
                )
                .unwrap();
            }),
        ),
        (
            "overlarge_summary",
            Box::new(|conn| {
                conn.execute(
                    "UPDATE project_memories SET summary = ?1",
                    params!["x".repeat(MAX_MEMORY_SUMMARY_CHARS + 1)],
                )
                .unwrap();
            }),
        ),
        (
            "bad_id",
            Box::new(|conn| {
                conn.execute("UPDATE project_memories SET memory_id = 'wc_mem_bad'", [])
                    .unwrap();
            }),
        ),
        (
            "bad_definition_hash",
            Box::new(|conn| {
                conn.execute(
                    "UPDATE project_memories SET definition_hash = ?1",
                    params![format!("wc_memdef_{}", "0".repeat(64))],
                )
                .unwrap();
            }),
        ),
        (
            "bad_revision",
            Box::new(|conn| {
                conn.execute(
                    "UPDATE project_memories SET revision = ?1",
                    params![format!("wc_memrev_{}", "0".repeat(64))],
                )
                .unwrap();
            }),
        ),
        (
            "malformed_tags",
            Box::new(|conn| {
                conn.execute("UPDATE project_memories SET tags_json = '{'", [])
                    .unwrap();
            }),
        ),
        (
            "too_many_tags",
            Box::new(|conn| {
                conn.execute(
                    "UPDATE project_memories SET tags_json = ?1",
                    params![serde_json::to_string(
                        &(0..=MAX_MEMORY_TAGS)
                            .map(|i| format!("t{i}"))
                            .collect::<Vec<_>>()
                    )
                    .unwrap()],
                )
                .unwrap();
            }),
        ),
        (
            "noncanonical_tags",
            Box::new(|conn| {
                conn.execute(
                    "UPDATE project_memories SET tags_json = '[\"z\",\"a\",\"a\"]'",
                    [],
                )
                .unwrap();
            }),
        ),
        (
            "zero_generation",
            Box::new(|conn| {
                conn.execute_batch("PRAGMA ignore_check_constraints = ON; UPDATE project_memories SET generation = 0; PRAGMA ignore_check_constraints = OFF;").unwrap();
            }),
        ),
    ];
    for (label, corrupt) in cases {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join(format!("{label}.db"))).unwrap();
        let scope = scope('f');
        db.set_project_memory(&scope, input("policy", "summary"))
            .unwrap();
        {
            let conn = db.conn_for_tests();
            corrupt(&conn);
        }
        assert_eq!(
            db.get_project_memory(&scope, "policy").unwrap_err().code(),
            "memory_store_unavailable",
            "{label} get"
        );
        assert_eq!(
            db.list_project_memories(&scope).unwrap_err().code(),
            "memory_store_unavailable",
            "{label} list"
        );
    }
}

#[test]
fn attributed_scope_and_provenance_track_real_mutations_without_noop_churn() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("memory.db");
    let db = Database::open(&path).unwrap();
    let memory_scope_id = scope('c');
    let scope_meta = scope_attribution("agent:runner:demo", "runner", 'a');
    let creator = principal("shared-key", '1');
    let updater = principal("oauth2", '2');

    let created = db
        .set_project_memory_attributed(
            &memory_scope_id,
            &scope_meta,
            &creator,
            input("policy-a", "original"),
        )
        .unwrap();
    assert_eq!(created.record.created_by_kind, creator.kind);
    assert_eq!(created.record.updated_by_kind, creator.kind);
    assert_eq!(
        created.record.created_by_principal_digest.as_deref(),
        Some(creator.principal_digest.as_str())
    );
    let first_scope = db
        .get_project_memory_scope(&memory_scope_id)
        .unwrap()
        .unwrap()
        .scope;
    assert_eq!(first_scope.identity_state, MEMORY_SCOPE_IDENTITY_ATTRIBUTED);
    assert_eq!(
        first_scope.project_runtime_id.as_deref(),
        Some(scope_meta.project_runtime_id.as_str())
    );
    assert_eq!(
        first_scope.runner_client_id.as_deref(),
        Some(scope_meta.runner_client_id.as_str())
    );
    assert_eq!(
        first_scope.root_fingerprint.as_deref(),
        Some(scope_meta.root_fingerprint.as_str())
    );

    let noop = db
        .set_project_memory_attributed(
            &memory_scope_id,
            &scope_meta,
            &updater,
            input("policy-a", "original"),
        )
        .unwrap();
    assert!(!noop.state_changed);
    assert_eq!(noop.record.updated_by_kind, creator.kind);
    assert_eq!(noop.record.revision, created.record.revision);
    let after_noop_scope = db
        .get_project_memory_scope(&memory_scope_id)
        .unwrap()
        .unwrap()
        .scope;
    assert_eq!(
        after_noop_scope.last_mutated_at_unix_ms,
        first_scope.last_mutated_at_unix_ms
    );

    let second = db
        .set_project_memory_attributed(
            &memory_scope_id,
            &scope_meta,
            &creator,
            input("policy-b", "second"),
        )
        .unwrap();
    let shared_scope = db
        .get_project_memory_scope(&memory_scope_id)
        .unwrap()
        .unwrap();
    assert_eq!(shared_scope.memories.len(), 2);

    let mut update = input("policy-a", "changed");
    update.expected_revision = Some(created.record.revision.clone());
    let changed = db
        .set_project_memory_attributed(&memory_scope_id, &scope_meta, &updater, update)
        .unwrap();
    assert!(changed.state_changed);
    assert_eq!(changed.record.created_by_kind, creator.kind);
    assert_eq!(changed.record.updated_by_kind, updater.kind);
    assert_eq!(
        changed.record.created_by_principal_digest,
        created.record.created_by_principal_digest
    );
    assert_eq!(
        changed.record.updated_by_principal_digest.as_deref(),
        Some(updater.principal_digest.as_str())
    );

    // Scope attribution and provenance are durable Control data, not process
    // memory. Reopening the same SQLite store must preserve them exactly.
    drop(db);
    let db = Database::open(&path).unwrap();
    let restarted = db
        .get_project_memory_scope(&memory_scope_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        restarted.scope.identity_state,
        MEMORY_SCOPE_IDENTITY_ATTRIBUTED
    );
    let restarted_policy = restarted
        .memories
        .iter()
        .find(|record| record.memory_key == "policy-a")
        .unwrap();
    assert_eq!(restarted_policy.created_by_kind, creator.kind);
    assert_eq!(restarted_policy.updated_by_kind, updater.kind);
    assert_eq!(
        restarted_policy.updated_by_principal_digest.as_deref(),
        Some(updater.principal_digest.as_str())
    );

    db.delete_project_memory_attributed(
        &memory_scope_id,
        &scope_meta,
        "policy-b",
        &second.record.revision,
    )
    .unwrap();
    assert_eq!(
        db.get_project_memory_scope(&memory_scope_id)
            .unwrap()
            .unwrap()
            .memories
            .len(),
        1
    );
    db.delete_project_memory_attributed(
        &memory_scope_id,
        &scope_meta,
        "policy-a",
        &changed.record.revision,
    )
    .unwrap();
    assert!(db
        .get_project_memory_scope(&memory_scope_id)
        .unwrap()
        .is_none());
}

#[test]
fn purge_is_catalog_fenced_atomic_and_desired_state_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("purge.db")).unwrap();
    let memory_scope_id = scope('e');
    let scope_meta = scope_attribution("agent:runner:detached", "runner", 'c');
    let actor = principal("bootstrap", '4');
    let first = db
        .set_project_memory_attributed(&memory_scope_id, &scope_meta, &actor, input("a", "A"))
        .unwrap();
    let second = db
        .set_project_memory_attributed(&memory_scope_id, &scope_meta, &actor, input("b", "B"))
        .unwrap();
    let initial = db
        .get_project_memory_scope(&memory_scope_id)
        .unwrap()
        .unwrap();
    let stale_catalog = memory_catalog_revision(&initial.memories);

    let mut update = input("a", "A2");
    update.expected_revision = Some(first.record.revision);
    let updated = db
        .set_project_memory_attributed(&memory_scope_id, &scope_meta, &actor, update)
        .unwrap();
    assert_eq!(
        db.purge_project_memory_scope(&memory_scope_id, &stale_catalog)
            .unwrap_err()
            .code(),
        "memory_scope_changed"
    );
    assert_eq!(
        db.get_project_memory_scope(&memory_scope_id)
            .unwrap()
            .unwrap()
            .memories
            .len(),
        2
    );

    let before_recreate = db
        .get_project_memory_scope(&memory_scope_id)
        .unwrap()
        .unwrap();
    let recreate_stale = memory_catalog_revision(&before_recreate.memories);
    db.delete_project_memory_attributed(
        &memory_scope_id,
        &scope_meta,
        "b",
        &second.record.revision,
    )
    .unwrap();
    let recreated = db
        .set_project_memory_attributed(&memory_scope_id, &scope_meta, &actor, input("b", "B"))
        .unwrap();
    assert_ne!(recreated.record.memory_id, second.record.memory_id);
    assert_eq!(
        db.purge_project_memory_scope(&memory_scope_id, &recreate_stale)
            .unwrap_err()
            .code(),
        "memory_scope_changed"
    );

    let current = db
        .get_project_memory_scope(&memory_scope_id)
        .unwrap()
        .unwrap();
    let current_catalog = memory_catalog_revision(&current.memories);
    db.conn_for_tests()
        .execute_batch(
            "CREATE TRIGGER fail_scope_metadata_delete BEFORE DELETE ON project_memory_scopes
             BEGIN SELECT RAISE(ABORT, 'forced scope purge failure'); END;",
        )
        .unwrap();
    assert_eq!(
        db.purge_project_memory_scope(&memory_scope_id, &current_catalog)
            .unwrap_err()
            .code(),
        "memory_store_unavailable"
    );
    let after_failed_purge = db
        .get_project_memory_scope(&memory_scope_id)
        .unwrap()
        .unwrap();
    assert_eq!(after_failed_purge.memories.len(), 2);
    db.conn_for_tests()
        .execute_batch("DROP TRIGGER fail_scope_metadata_delete;")
        .unwrap();
    let purged = db
        .purge_project_memory_scope(&memory_scope_id, &current_catalog)
        .unwrap();
    assert!(purged.purged && purged.state_changed);
    assert_eq!(purged.purged_count, 2);
    assert!(db
        .get_project_memory_scope(&memory_scope_id)
        .unwrap()
        .is_none());
    assert!(db
        .get_project_memory(&memory_scope_id, "a")
        .unwrap()
        .is_none());
    let repeated = db
        .purge_project_memory_scope(&memory_scope_id, &current_catalog)
        .unwrap();
    assert!(!repeated.purged && !repeated.state_changed);
    assert_eq!(repeated.purged_count, 0);
    let _ = updated;
}

#[test]
fn corrupted_scope_or_provenance_metadata_fails_closed() {
    for (label, corrupt) in [
        (
            "scope_identity",
            "PRAGMA ignore_check_constraints = ON;
             UPDATE project_memory_scopes SET identity_state = 'bogus';
             PRAGMA ignore_check_constraints = OFF;",
        ),
        (
            "scope_root",
            "UPDATE project_memory_scopes SET root_fingerprint = 'wc_memroot_bad';",
        ),
        (
            "scope_project_too_long",
            "UPDATE project_memory_scopes SET project_runtime_id = printf('%0513d', 0);",
        ),
        (
            "scope_runner_too_long",
            "UPDATE project_memory_scopes SET runner_client_id = printf('%0129d', 0);",
        ),
        (
            "scope_timestamps",
            "UPDATE project_memory_scopes
             SET last_mutated_at_unix_ms = created_at_unix_ms - 1;",
        ),
        (
            "provenance_digest",
            "UPDATE project_memories
             SET created_by_kind = 'shared-key', created_by_principal_digest = 'wc_memprincipal_bad';",
        ),
        (
            "provenance_kind",
            "UPDATE project_memories SET created_by_kind = 'bogus';",
        ),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join(format!("{label}.db"))).unwrap();
        let memory_scope_id = scope('f');
        db.set_project_memory(&memory_scope_id, input("policy", "summary"))
            .unwrap();
        db.conn_for_tests().execute_batch(corrupt).unwrap();
        assert_eq!(
            db.get_project_memory_scope(&memory_scope_id)
                .unwrap_err()
                .code(),
            "memory_store_unavailable",
            "{label} scope"
        );
        assert_eq!(
            db.list_project_memories(&memory_scope_id)
                .unwrap_err()
                .code(),
            "memory_store_unavailable",
            "{label} memory"
        );
    }
}

#[test]
fn lifecycle_scope_list_fails_closed_when_scope_metadata_is_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("missing-scope-row.db")).unwrap();
    let memory_scope_id = scope('e');
    db.set_project_memory(&memory_scope_id, input("policy", "summary"))
        .unwrap();
    db.conn_for_tests()
        .execute(
            "DELETE FROM project_memory_scopes WHERE memory_scope_id = ?1",
            params![memory_scope_id],
        )
        .unwrap();
    assert_eq!(
        db.list_project_memory_scopes(0, 10).unwrap_err().code(),
        "memory_store_unavailable",
        "lifecycle inventory must not silently omit Memory rows whose scope metadata is missing"
    );
}

#[test]
fn unsupported_memory_schema_fails_closed_without_partial_indexes() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("broken-memory-schema.db");
    let raw = Connection::open(&path).unwrap();
    raw.execute_batch(
        "CREATE TABLE project_memories (memory_id TEXT PRIMARY KEY);
         CREATE TABLE sentinel (value TEXT NOT NULL);
         INSERT INTO sentinel VALUES ('keep-me');",
    )
    .unwrap();
    drop(raw);

    assert!(Database::open(&path).is_err());
    let raw = Connection::open(&path).unwrap();
    let sentinel: String = raw
        .query_row("SELECT value FROM sentinel", [], |row| row.get(0))
        .unwrap();
    assert_eq!(sentinel, "keep-me");
    let index_count: i64 = raw
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type='index' AND name IN (
                 'idx_project_memories_scope_key',
                 'idx_project_memories_scope_bootstrap'
             )",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        index_count, 0,
        "unsupported schema must not leave partial indexes"
    );
}

#[test]
fn post_v039_legacy_memory_constraints_fail_closed_at_open() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("legacy-memory-constraints.db");
    let raw = Connection::open(&path).unwrap();
    raw.execute_batch(
        "CREATE TABLE project_memories (
            memory_id TEXT PRIMARY KEY,
            memory_scope_id TEXT NOT NULL,
            memory_key TEXT NOT NULL,
            summary TEXT NOT NULL,
            body TEXT NOT NULL,
            priority TEXT NOT NULL CHECK(priority IN ('high', 'normal', 'low')),
            bootstrap INTEGER NOT NULL CHECK(bootstrap IN (0, 1)),
            tags_json TEXT NOT NULL,
            definition_hash TEXT NOT NULL,
            generation INTEGER NOT NULL CHECK(generation >= 1),
            revision TEXT NOT NULL,
            created_at_unix_ms INTEGER NOT NULL,
            updated_at_unix_ms INTEGER NOT NULL,
            created_by_kind TEXT NOT NULL,
            created_by_principal_digest TEXT,
            updated_by_kind TEXT NOT NULL,
            updated_by_principal_digest TEXT,
            UNIQUE(memory_scope_id, memory_key)
         );
         CREATE TABLE project_memory_scopes (
            memory_scope_id TEXT PRIMARY KEY,
            identity_state TEXT NOT NULL CHECK(identity_state IN ('attributed', 'legacy_unattributed')),
            project_runtime_id TEXT,
            runner_client_id TEXT,
            root_fingerprint TEXT,
            created_at_unix_ms INTEGER NOT NULL,
            last_mutated_at_unix_ms INTEGER NOT NULL,
            CHECK(
                (identity_state = 'legacy_unattributed'
                    AND project_runtime_id IS NULL
                    AND runner_client_id IS NULL
                    AND root_fingerprint IS NULL)
                OR
                (identity_state = 'attributed'
                    AND project_runtime_id IS NOT NULL
                    AND runner_client_id IS NOT NULL
                    AND root_fingerprint IS NOT NULL)
            )
         );
         CREATE TABLE sentinel (value TEXT NOT NULL);
         INSERT INTO sentinel VALUES ('keep-me');",
    )
    .unwrap();
    drop(raw);

    let error = match Database::open(&path) {
        Ok(_) => panic!("legacy post-v0.3.9 Memory constraints must be rejected at open"),
        Err(error) => error,
    };
    assert!(
        format!("{error:#}").contains("unsupported project Memory schema shape"),
        "unexpected error: {error:#}"
    );

    let raw = Connection::open(&path).unwrap();
    let sentinel: String = raw
        .query_row("SELECT value FROM sentinel", [], |row| row.get(0))
        .unwrap();
    assert_eq!(sentinel, "keep-me");
    let index_count: i64 = raw
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type='index' AND name IN (
                 'idx_project_memories_scope_key',
                 'idx_project_memories_scope_bootstrap'
             )",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        index_count, 0,
        "rejected development schema must not leave partial Memory indexes"
    );
}

#[test]
fn memory_schema_initialization_preserves_unrelated_database_data() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("existing.db");
    let raw = Connection::open(&path).unwrap();
    raw.execute_batch(
        "CREATE TABLE sentinel (value TEXT NOT NULL); INSERT INTO sentinel VALUES ('keep-me');",
    )
    .unwrap();
    drop(raw);

    let db = Database::open(&path).unwrap();
    let conn = db.conn_for_tests();
    let value: String = conn
        .query_row("SELECT value FROM sentinel", [], |row| row.get(0))
        .unwrap();
    assert_eq!(value, "keep-me");
    let memory_table: String = conn
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='project_memories'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(memory_table, "project_memories");
}

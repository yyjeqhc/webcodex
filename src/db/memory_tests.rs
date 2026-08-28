use super::*;
use crate::db::memory::{memory_revision, MemoryPriority, MemorySetInput};
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
        memory_revision("k", "s", "b", MemoryPriority::High, true, &tags_a),
        memory_revision("k", "s", "b", MemoryPriority::High, true, &canonical_b)
    );
    assert_ne!(
        memory_revision("k", "s", "b", MemoryPriority::High, true, &tags_a),
        memory_revision("k", "s", "changed", MemoryPriority::High, true, &tags_a)
    );
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
        let revision = memory_revision("seed", "s", "", MemoryPriority::Normal, false, &[]);
        for index in 0..MAX_MEMORIES_GLOBAL {
            tx.execute(
                "INSERT INTO project_memories
                 (memory_id, memory_scope_id, memory_key, summary, body, priority, bootstrap,
                  tags_json, revision, created_at_unix_ms, updated_at_unix_ms)
                 VALUES (?1, ?2, ?3, 's', '', 'normal', 0, '[]', ?4, 1, 1)",
                params![
                    format!("wc_mem_{index:032x}"),
                    format!("wc_memscope_{:064x}", index + 100),
                    format!("k{index}"),
                    revision,
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
fn memory_schema_migration_fails_closed_without_partial_indexes() {
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
        "failed migration must not leave partial indexes"
    );
}

#[test]
fn memory_schema_is_additive_and_preserves_existing_database_data() {
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

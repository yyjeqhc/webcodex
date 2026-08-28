use super::*;
use crate::db::memory::{
    legacy_memory_revision_v1, memory_definition_hash, memory_state_revision, MemoryPriority,
    MemorySetInput,
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
                  created_at_unix_ms, updated_at_unix_ms)
                 VALUES (?1, ?2, ?3, 's', '', 'normal', 0, '[]', ?4, 1, ?5, 1, 1)",
                params![
                    memory_id,
                    memory_scope_id,
                    memory_key,
                    definition_hash,
                    revision
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
fn current_203_memory_row_migrates_to_generation_one_state_revision() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("memory-v1.db");
    let raw = Connection::open(&path).unwrap();
    raw.execute_batch(
        "CREATE TABLE project_memories (
            memory_id TEXT PRIMARY KEY, memory_scope_id TEXT NOT NULL, memory_key TEXT NOT NULL,
            summary TEXT NOT NULL, body TEXT NOT NULL, priority TEXT NOT NULL,
            bootstrap INTEGER NOT NULL, tags_json TEXT NOT NULL, revision TEXT NOT NULL,
            created_at_unix_ms INTEGER NOT NULL, updated_at_unix_ms INTEGER NOT NULL,
            UNIQUE(memory_scope_id, memory_key));
         CREATE INDEX idx_project_memories_scope_key ON project_memories(memory_scope_id, memory_key);
         CREATE INDEX idx_project_memories_scope_bootstrap ON project_memories(memory_scope_id, bootstrap, priority, memory_key);",
    )
    .unwrap();
    let memory_scope_id = scope('d');
    let memory_id = "wc_mem_0123456789abcdef0123456789abcdef";
    let tags = vec!["architecture".to_string(), "stable".to_string()];
    let old_revision = legacy_memory_revision_v1(
        "policy",
        "summary",
        "body",
        MemoryPriority::High,
        true,
        &tags,
    );
    raw.execute(
        "INSERT INTO project_memories VALUES (?1, ?2, 'policy', 'summary', 'body', 'high', 1, ?3, ?4, 10, 20)",
        params![memory_id, memory_scope_id, serde_json::to_string(&tags).unwrap(), old_revision],
    )
    .unwrap();
    drop(raw);

    let db = Database::open(&path).unwrap();
    let record = db
        .get_project_memory(&memory_scope_id, "policy")
        .unwrap()
        .unwrap();
    assert_eq!(record.memory_id, memory_id);
    assert_eq!(record.summary, "summary");
    assert_eq!(record.body, "body");
    assert_eq!(record.generation, 1);
    assert!(record.definition_hash.starts_with("wc_memdef_"));
    assert_eq!(
        record.revision,
        memory_state_revision(&memory_scope_id, memory_id, 1, &record.definition_hash)
    );
}

#[test]
fn overlarge_203_memory_row_migration_fails_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("memory-v1-overlarge.db");
    let raw = Connection::open(&path).unwrap();
    raw.execute_batch(
        "CREATE TABLE project_memories (
            memory_id TEXT PRIMARY KEY, memory_scope_id TEXT NOT NULL, memory_key TEXT NOT NULL,
            summary TEXT NOT NULL, body TEXT NOT NULL, priority TEXT NOT NULL,
            bootstrap INTEGER NOT NULL, tags_json TEXT NOT NULL, revision TEXT NOT NULL,
            created_at_unix_ms INTEGER NOT NULL, updated_at_unix_ms INTEGER NOT NULL,
            UNIQUE(memory_scope_id, memory_key));",
    )
    .unwrap();
    let memory_scope_id = scope('e');
    let memory_id = "wc_mem_0123456789abcdef0123456789abcdef";
    let body = "x".repeat(MAX_MEMORY_BODY_BYTES + 1);
    let old_revision = legacy_memory_revision_v1(
        "policy",
        "summary",
        &body,
        MemoryPriority::Normal,
        false,
        &[],
    );
    raw.execute(
        "INSERT INTO project_memories VALUES (?1, ?2, 'policy', 'summary', ?3, 'normal', 0, '[]', ?4, 1, 1)",
        params![memory_id, memory_scope_id, body, old_revision],
    )
    .unwrap();
    drop(raw);

    assert!(Database::open(&path).is_err());
    let raw = Connection::open(&path).unwrap();
    let columns = raw
        .prepare("PRAGMA table_info(project_memories)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(!columns.iter().any(|column| column == "definition_hash"));
    assert!(!columns.iter().any(|column| column == "generation"));
}

#[test]
fn malformed_203_memory_row_migration_rolls_back() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("memory-v1-bad.db");
    let raw = Connection::open(&path).unwrap();
    raw.execute_batch(
        "CREATE TABLE project_memories (
            memory_id TEXT PRIMARY KEY, memory_scope_id TEXT NOT NULL, memory_key TEXT NOT NULL,
            summary TEXT NOT NULL, body TEXT NOT NULL, priority TEXT NOT NULL,
            bootstrap INTEGER NOT NULL, tags_json TEXT NOT NULL, revision TEXT NOT NULL,
            created_at_unix_ms INTEGER NOT NULL, updated_at_unix_ms INTEGER NOT NULL,
            UNIQUE(memory_scope_id, memory_key));
         CREATE INDEX idx_project_memories_scope_key ON project_memories(memory_scope_id, memory_key);
         CREATE INDEX idx_project_memories_scope_bootstrap ON project_memories(memory_scope_id, bootstrap, priority, memory_key);",
    )
    .unwrap();
    raw.execute(
        "INSERT INTO project_memories VALUES (?1, ?2, 'policy', 'summary', 'body', 'normal', 0, '[]', ?3, 1, 1)",
        params![
            "wc_mem_0123456789abcdef0123456789abcdef",
            scope('e'),
            format!("wc_memrev_{}", "0".repeat(64))
        ],
    )
    .unwrap();
    drop(raw);

    assert!(Database::open(&path).is_err());
    let raw = Connection::open(&path).unwrap();
    let columns = raw
        .prepare("PRAGMA table_info(project_memories)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(!columns.iter().any(|column| column == "definition_hash"));
    assert!(!columns.iter().any(|column| column == "generation"));
    let count: i64 = raw
        .query_row("SELECT COUNT(*) FROM project_memories", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 1);
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

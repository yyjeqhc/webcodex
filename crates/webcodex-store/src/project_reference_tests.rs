use super::Database;

fn root(hex: char) -> String {
    format!("wc_projroot_{}", hex.to_string().repeat(64))
}

thread_local! {
    static REFERENCE_SQL: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn capture_reference_statement(sql: &str) {
    REFERENCE_SQL.with(|statements| statements.borrow_mut().push(sql.to_string()));
}

#[test]
#[allow(deprecated)]
fn project_reference_batches_are_bounded_atomic_and_warm_read_only() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("batch-refs.db");
    let db = Database::open(&path).unwrap();
    let one = root('1');
    let two = root('2');
    let input = [
        ("agent:r:a", one.as_str()),
        ("agent:r:b", two.as_str()),
        ("agent:r:a", one.as_str()),
        ("agent:r:invalid", "not-a-root"),
    ];
    {
        let mut conn = db.lock_connection(super::StoreDomain::ProjectReference);
        conn.trace(Some(capture_reference_statement));
    }
    REFERENCE_SQL.with(|statements| statements.borrow_mut().clear());
    let first = db.get_or_create_project_references("a", &input, 1).unwrap();
    assert_eq!(
        first[0], first[2],
        "duplicate identities share one allocation"
    );
    assert_eq!(first[0].as_ref().unwrap().ref_index, 1);
    assert_eq!(first[1].as_ref().unwrap().ref_index, 2);
    assert!(
        first[3].is_none(),
        "invalid optional identity has no mapping"
    );
    let cold_sql = REFERENCE_SQL.with(|statements| statements.take());
    assert_eq!(
        cold_sql
            .iter()
            .filter(|sql| sql.starts_with("BEGIN"))
            .count(),
        1
    );
    assert_eq!(
        cold_sql
            .iter()
            .filter(|sql| sql.starts_with("COMMIT"))
            .count(),
        1
    );
    assert_eq!(
        cold_sql
            .iter()
            .filter(|sql| sql.starts_with("INSERT"))
            .count(),
        2
    );

    let warm = db.get_or_create_project_references("a", &input, 2).unwrap();
    assert_eq!(warm, first);
    let warm_sql = REFERENCE_SQL.with(|statements| statements.take());
    assert_eq!(
        warm_sql.len(),
        1,
        "warm batch performs one SELECT: {warm_sql:?}"
    );
    assert!(warm_sql[0].starts_with("SELECT"));
    assert!(!warm_sql[0].contains("BEGIN"));

    let other = db
        .get_or_create_project_references("b", &input[..1], 3)
        .unwrap();
    assert_eq!(other[0].as_ref().unwrap().ref_index, 1);
    let rebound = db
        .get_or_create_project_references("a", &[("agent:r:a", &two)], 4)
        .unwrap();
    assert_eq!(rebound[0].as_ref().unwrap().ref_index, 3);
    assert_eq!(db.lookup_project_reference("a", 1).unwrap(), first[0]);
    {
        let mut conn = db.lock_connection(super::StoreDomain::ProjectReference);
        conn.trace(None);
    }
    drop(db);
    let reopened = Database::open(&path).unwrap();
    assert_eq!(
        reopened
            .get_or_create_project_references("a", &input, 5)
            .unwrap(),
        first
    );
    assert_eq!(
        reopened.lookup_project_reference("a", 3).unwrap(),
        rebound[0]
    );
    assert!(reopened.lookup_project_reference("b", 2).unwrap().is_none());
    let too_many = vec![("agent:r:a", one.as_str()); 2_001];
    assert!(matches!(
        reopened.get_or_create_project_references("a", &too_many, 6),
        Err(super::ProjectReferenceStoreError::InvalidInput),
    ));
}

#[test]
fn concurrent_project_reference_batches_share_one_durable_identity() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("concurrent-refs.db");
    let first_db = Database::open(&path).unwrap();
    let second_db = Database::open(&path).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let first_gate = barrier.clone();
    let first = std::thread::spawn(move || {
        first_gate.wait();
        first_db
            .get_or_create_project_references("a", &[("agent:r:a", &root('1'))], 1)
            .unwrap()
    });
    barrier.wait();
    let second = second_db
        .get_or_create_project_references("a", &[("agent:r:a", &root('1'))], 1)
        .unwrap();
    assert_eq!(first.join().unwrap(), second);
}

#[test]
fn project_references_are_stable_isolated_and_restart_safe() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("project-refs.db");

    let db = Database::open(&path).unwrap();
    let first = db
        .get_or_create_project_reference("principal-a", "agent:special:webcodex", &root('1'), 1)
        .unwrap();
    assert_eq!(first.ref_index, 1);

    let replay = db
        .get_or_create_project_reference("principal-a", "agent:special:webcodex", &root('1'), 2)
        .unwrap();
    assert_eq!(replay, first);

    let second_project = db
        .get_or_create_project_reference("principal-a", "agent:oe:webcodex", &root('2'), 3)
        .unwrap();
    assert_eq!(second_project.ref_index, 2);

    let other_principal = db
        .get_or_create_project_reference("principal-b", "agent:special:webcodex", &root('1'), 4)
        .unwrap();
    assert_eq!(other_principal.ref_index, 1);

    let reincarnated = db
        .get_or_create_project_reference("principal-a", "agent:special:webcodex", &root('3'), 5)
        .unwrap();
    assert_eq!(reincarnated.ref_index, 3);
    drop(db);

    let reopened = Database::open(&path).unwrap();
    assert_eq!(
        reopened
            .lookup_project_reference("principal-a", 1)
            .unwrap()
            .unwrap(),
        first
    );
    assert_eq!(
        reopened
            .lookup_project_reference("principal-a", 2)
            .unwrap()
            .unwrap(),
        second_project
    );
    assert_eq!(
        reopened
            .lookup_project_reference("principal-a", 3)
            .unwrap()
            .unwrap(),
        reincarnated
    );
    assert_eq!(
        reopened
            .lookup_project_reference("principal-b", 1)
            .unwrap()
            .unwrap(),
        other_principal
    );
    assert!(
        reopened
            .lookup_project_reference("principal-b", 2)
            .unwrap()
            .is_none(),
        "principal namespaces must not expose another principal's mapping"
    );
}

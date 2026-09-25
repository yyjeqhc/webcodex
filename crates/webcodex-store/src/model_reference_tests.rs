use super::Database;
use webcodex_core::model_reference::ModelReferenceKind;

fn root(hex: char) -> String {
    format!("wc_projroot_{}", hex.to_string().repeat(64))
}

fn fingerprint(hex: char) -> String {
    hex.to_string().repeat(64)
}

#[test]
fn typed_model_references_are_stable_isolated_and_restart_safe() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("model-refs.db");

    let db = Database::open(&path).unwrap();
    let first = db
        .get_or_create_model_reference(
            "principal-a",
            ModelReferenceKind::Project,
            "agent:special:webcodex",
            &root('1'),
            1,
        )
        .unwrap();
    assert_eq!(first.ref_index, 1);

    let replay = db
        .get_or_create_model_reference(
            "principal-a",
            ModelReferenceKind::Project,
            "agent:special:webcodex",
            &root('1'),
            2,
        )
        .unwrap();
    assert_eq!(replay, first);

    let session = db
        .get_or_create_model_reference(
            "principal-a",
            ModelReferenceKind::Session,
            "wc_sess_1234567890abcdef",
            &fingerprint('a'),
            3,
        )
        .unwrap();
    assert_eq!(
        session.ref_index, 1,
        "reference kinds have independent short-index namespaces"
    );

    let other_principal = db
        .get_or_create_model_reference(
            "principal-b",
            ModelReferenceKind::Project,
            "agent:special:webcodex",
            &root('1'),
            4,
        )
        .unwrap();
    assert_eq!(other_principal.ref_index, 1);

    let reincarnated = db
        .get_or_create_model_reference(
            "principal-a",
            ModelReferenceKind::Project,
            "agent:special:webcodex",
            &root('3'),
            5,
        )
        .unwrap();
    assert_eq!(reincarnated.ref_index, 2);
    drop(db);

    let reopened = Database::open(&path).unwrap();
    assert_eq!(
        reopened
            .lookup_model_reference("principal-a", ModelReferenceKind::Project, 1)
            .unwrap()
            .unwrap(),
        first
    );
    assert_eq!(
        reopened
            .lookup_model_reference("principal-a", ModelReferenceKind::Project, 2)
            .unwrap()
            .unwrap(),
        reincarnated
    );
    assert_eq!(
        reopened
            .lookup_model_reference("principal-a", ModelReferenceKind::Session, 1)
            .unwrap()
            .unwrap(),
        session
    );
    assert!(
        reopened
            .lookup_model_reference("principal-b", ModelReferenceKind::Session, 1)
            .unwrap()
            .is_none(),
        "principal and kind namespaces must not leak mappings"
    );
}

#[test]
fn released_project_reference_table_remains_authoritative() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("project-ref-compat.db");
    let db = Database::open(&path).unwrap();
    db.conn_for_tests()
        .execute(
            "INSERT INTO project_references VALUES (?1, 7, ?2, ?3, 11)",
            rusqlite::params!["principal-a", "agent:runner:repo", root('a')],
        )
        .unwrap();

    let retained = db
        .lookup_model_reference("principal-a", ModelReferenceKind::Project, 7)
        .unwrap()
        .unwrap();
    assert_eq!(retained.canonical_id, "agent:runner:repo");
    assert_eq!(retained.incarnation_fingerprint, root('a'));

    let next = db
        .get_or_create_model_reference(
            "principal-a",
            ModelReferenceKind::Project,
            "agent:runner:next",
            &root('b'),
            12,
        )
        .unwrap();
    assert_eq!(next.ref_index, 8);

    let raw: (String, String) = db
        .conn_for_tests()
        .query_row(
            "SELECT canonical_project_id, root_fingerprint
             FROM project_references WHERE principal_key=?1 AND ref_index=8",
            ["principal-a"],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(raw.0, "agent:runner:next");
    assert_eq!(raw.1, root('b'));
}

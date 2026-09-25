use super::communication::CommunicationPrincipal;
use super::Database;

fn principal_with_kind(kind: &str, hex: char) -> CommunicationPrincipal {
    CommunicationPrincipal {
        kind: kind.to_string(),
        digest: format!("wc_commprincipal_{}", hex.to_string().repeat(64)),
    }
}

fn principal(hex: char) -> CommunicationPrincipal {
    principal_with_kind("managed-user", hex)
}

const AGENT: &str = "wc_dagent_qqqqqqqqqqqqqqqq";
const ENDPOINT_A: &str = "wc_endpoint_u7u7u7u7u7u7u7u7";
const ENDPOINT_B: &str = "wc_endpoint_3d3d3d3d3d3d3d3d";

#[test]
fn agent_continuation_references_pin_generation_and_do_not_retarget() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("continuation-refs.db");
    let alice = principal('a');
    let bob = principal('b');

    let db = Database::open(&path).unwrap();
    let first = db
        .get_or_create_agent_continuation_reference(&alice, AGENT, ENDPOINT_A, 1, 10)
        .unwrap();
    assert_eq!(first.ref_index, 1);
    let replay = db
        .get_or_create_agent_continuation_reference(&alice, AGENT, ENDPOINT_A, 1, 99)
        .unwrap();
    assert_eq!(replay, first);
    let created_at: i64 = db
        .conn_for_tests()
        .query_row(
            "SELECT created_at_unix_ms FROM wc_agent_continuation_references
             WHERE principal_kind = ?1 AND principal_digest = ?2 AND ref_index = 1",
            [alice.kind.as_str(), alice.digest.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(created_at, 10, "reissuing a tuple must not rewrite the row");

    let rotated = db
        .get_or_create_agent_continuation_reference(&alice, AGENT, ENDPOINT_B, 2, 20)
        .unwrap();
    assert_eq!(rotated.ref_index, 2);
    assert_eq!(
        db.lookup_agent_continuation_reference(&alice, 1)
            .unwrap()
            .unwrap(),
        first,
        "the old index stays pinned to generation 1"
    );

    let bob_same_tuple = db
        .get_or_create_agent_continuation_reference(&bob, AGENT, ENDPOINT_A, 1, 30)
        .unwrap();
    assert_eq!(bob_same_tuple.ref_index, 1);
    assert!(
        db.lookup_agent_continuation_reference(&bob, 2)
            .unwrap()
            .is_none(),
        "another principal must not see Alice's later index"
    );
    let same_digest_other_kind = principal_with_kind("service", 'a');
    let other_kind_same_tuple = db
        .get_or_create_agent_continuation_reference(
            &same_digest_other_kind,
            AGENT,
            ENDPOINT_A,
            1,
            40,
        )
        .unwrap();
    assert_eq!(
        other_kind_same_tuple.ref_index, 1,
        "principal kind participates in the selector namespace"
    );
    assert!(
        db.lookup_agent_continuation_reference(&same_digest_other_kind, 2)
            .unwrap()
            .is_none(),
        "same digest text under another principal kind must remain isolated"
    );
    drop(db);

    let reopened = Database::open(&path).unwrap();
    assert_eq!(
        reopened
            .lookup_agent_continuation_reference(&alice, 2)
            .unwrap()
            .unwrap(),
        rotated
    );
    assert!(reopened
        .get_or_create_agent_continuation_reference(&alice, AGENT, ENDPOINT_A, 0, 1)
        .is_err());
    assert!(reopened
        .get_or_create_agent_continuation_reference(&alice, "agent", ENDPOINT_A, 1, 1)
        .is_err());
    assert!(reopened
        .lookup_agent_continuation_reference(&alice, 0)
        .is_err());
}

use super::super::communication::communication_principal;
use super::support::auth_context;
use crate::tool_runtime::ToolCall;
use crate::tool_runtime::ToolRuntime;
use crate::Database;
use serde_json::json;
use std::sync::Arc;

fn runtime_with_db(path: &std::path::Path) -> (Arc<Database>, ToolRuntime) {
    let db = Arc::new(Database::open(&path.to_path_buf()).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_communication_database(db.clone());
    (db, runtime)
}

fn create_agent(
    runtime: &ToolRuntime,
    auth: Option<&crate::auth::AuthContext>,
    key: &str,
) -> String {
    let result = runtime.create_agent_identity(
        auth,
        format!("agent-{key}"),
        format!("Agent {key}"),
        None,
        Vec::new(),
        key.to_string(),
    );
    assert!(result.success, "{:?}", result.output);
    result.output["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn rotate(
    runtime: &ToolRuntime,
    auth: Option<&crate::auth::AuthContext>,
    agent_id: &str,
    key: &str,
) -> serde_json::Value {
    let result = runtime.attach_agent_endpoint(
        auth,
        agent_id.to_string(),
        "ChatGPT".to_string(),
        None,
        key.to_string(),
    );
    assert!(result.success, "{:?}", result.output);
    result.output
}

fn present_ref(
    runtime: &ToolRuntime,
    auth: Option<&crate::auth::AuthContext>,
    agent_continuation_ref: &str,
) -> crate::tool_runtime::ToolResult {
    runtime.present_agent_continuation_with_selector(
        auth,
        Some(agent_continuation_ref.to_string()),
        None,
        None,
        None,
    )
}

#[test]
fn agent_continuation_ref_presents_the_pinned_tuple_and_fails_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let (db, runtime) = runtime_with_db(&tmp.path().join("refs.db"));
    let alice = auth_context(Some("alice"), false);
    let alice_other_key = {
        let mut auth = alice.clone();
        auth.api_key_id = Some("key-alice-other-session".to_string());
        auth
    };
    let bob = auth_context(Some("bob"), false);
    let agent_id = create_agent(&runtime, Some(&alice), "alice-agent");

    let first = rotate(&runtime, Some(&alice), &agent_id, "rotate-1");
    let first_ref = first["agent_continuation_ref"]
        .as_str()
        .unwrap()
        .to_string();
    let first_endpoint = first["endpoint"]["endpoint_id"]
        .as_str()
        .unwrap()
        .to_string();
    let first_generation = first["endpoint"]["controller_generation"].as_i64().unwrap();
    assert!(first_ref.starts_with("~ac"));

    let listed = runtime.list_agent_identities(Some(&alice), Some(agent_id.clone()), None, None);
    assert!(listed.success, "{:?}", listed.output);
    assert_eq!(
        listed.output["agents"][0]["agent_continuation_ref"],
        first_ref
    );

    let presented = present_ref(&runtime, Some(&alice), &first_ref);
    assert!(presented.success, "{:?}", presented.output);
    assert_eq!(presented.output["agent_continuation"]["agent_id"], agent_id);
    assert_eq!(
        presented.output["agent_continuation"]["endpoint_id"],
        first_endpoint
    );
    assert_eq!(
        presented.output["agent_continuation"]["controller_generation"],
        first_generation
    );
    let audit = crate::tool_runtime::tool_audit::session_log_result_for_tool(
        "present_agent_continuation",
        &presented.output,
    );
    assert_eq!(audit["agent_id"], agent_id);
    assert_eq!(audit["endpoint_id"], first_endpoint);
    assert_eq!(audit["controller_generation"], first_generation);
    let same_principal_other_key = present_ref(&runtime, Some(&alice_other_key), &first_ref);
    assert!(
        same_principal_other_key.success,
        "the ref is not an API-key or session credential: {:?}",
        same_principal_other_key.output
    );

    let explicit = runtime.present_agent_continuation_with_selector(
        Some(&alice),
        None,
        Some(agent_id.clone()),
        Some(first_endpoint.clone()),
        Some(first_generation),
    );
    assert!(explicit.success, "{:?}", explicit.output);

    let replay = rotate(&runtime, Some(&alice), &agent_id, "rotate-1");
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["agent_continuation_ref"], first_ref);
    assert_eq!(replay["endpoint"]["endpoint_id"], first_endpoint);

    let second = rotate(&runtime, Some(&alice), &agent_id, "rotate-2");
    let second_ref = second["agent_continuation_ref"]
        .as_str()
        .unwrap()
        .to_string();
    let second_endpoint = second["endpoint"]["endpoint_id"].as_str().unwrap();
    assert_ne!(second_ref, first_ref);
    assert_ne!(second_endpoint, first_endpoint);
    let replay_after_rotation = rotate(&runtime, Some(&alice), &agent_id, "rotate-1");
    assert_eq!(replay_after_rotation["replayed"], true);
    assert_eq!(replay_after_rotation["agent_continuation_ref"], first_ref);
    assert_eq!(
        replay_after_rotation["endpoint"]["endpoint_id"],
        first_endpoint
    );

    let first_index: u64 = first_ref.strip_prefix("~ac").unwrap().parse().unwrap();
    let alice_principal = communication_principal(Some(&alice)).unwrap();
    let pinned = db
        .lookup_agent_continuation_reference(&alice_principal, first_index)
        .unwrap()
        .unwrap();
    assert_eq!(pinned.endpoint_id, first_endpoint);
    assert_eq!(pinned.controller_generation, first_generation);

    let stale = present_ref(&runtime, Some(&alice), &first_ref);
    assert!(
        !stale.success,
        "stale ref must fail closed: {:?}",
        stale.output
    );
    assert_eq!(stale.output["error_kind"], "endpoint_expired");
    assert!(!stale.output.to_string().contains(second_endpoint));
    let stale_tuple = runtime.present_agent_continuation_with_selector(
        Some(&alice),
        None,
        Some(agent_id.clone()),
        Some(first_endpoint.clone()),
        Some(first_generation),
    );
    assert_eq!(stale.output["error_kind"], stale_tuple.output["error_kind"]);

    let current = present_ref(&runtime, Some(&alice), &second_ref);
    assert!(current.success, "{:?}", current.output);
    assert_eq!(
        current.output["agent_continuation"]["endpoint_id"],
        second_endpoint
    );
    let relisted = runtime.list_agent_identities(Some(&alice), Some(agent_id.clone()), None, None);
    assert_eq!(
        relisted.output["agents"][0]["agent_continuation_ref"],
        second_ref
    );

    let foreign = present_ref(&runtime, Some(&bob), &second_ref);
    assert!(!foreign.success, "{:?}", foreign.output);
    assert_eq!(
        foreign.output["error_kind"],
        "unknown_agent_continuation_ref"
    );
    let foreign_tuple = runtime.present_agent_continuation(
        Some(&bob),
        agent_id.clone(),
        second_endpoint.to_string(),
        second["endpoint"]["controller_generation"]
            .as_i64()
            .unwrap(),
    );
    assert!(!foreign_tuple.success);
    assert_eq!(foreign_tuple.output["error_kind"], "endpoint_not_found");

    for malformed in [
        "~ac0",
        "~ac01",
        "~p1",
        "~ac",
        "wc_dagent_qqqqqqqqqqqqqqqq",
        "~ac1 ",
    ] {
        let rejected = present_ref(&runtime, Some(&alice), malformed);
        assert!(
            !rejected.success,
            "{malformed} was accepted: {:?}",
            rejected.output
        );
        assert_eq!(
            rejected.output["error_kind"],
            "invalid_agent_continuation_ref"
        );
    }
    let unknown = present_ref(&runtime, Some(&alice), "~ac999");
    assert_eq!(
        unknown.output["error_kind"],
        "unknown_agent_continuation_ref"
    );

    let ambiguous = runtime.present_agent_continuation_with_selector(
        Some(&alice),
        Some(second_ref.clone()),
        Some(agent_id.clone()),
        None,
        None,
    );
    assert_eq!(
        ambiguous.output["error_kind"],
        "ambiguous_agent_continuation_selector"
    );
    let incomplete = runtime.present_agent_continuation_with_selector(
        Some(&alice),
        None,
        Some(agent_id.clone()),
        None,
        None,
    );
    assert_eq!(
        incomplete.output["error_kind"],
        "incomplete_agent_continuation_selector"
    );
    let session_field = ToolCall::from_tool_name(
        "present_agent_continuation",
        json!({
            "agent_continuation_ref": second_ref,
            "session_id": "wc_sess_0123456789abcdef0123456789abcdef"
        }),
    );
    assert!(
        session_field.is_err(),
        "a workflow session must not join the continuation selector"
    );

    let detached = runtime.detach_agent_endpoint(Some(&alice), second_endpoint.to_string());
    assert!(detached.success, "{:?}", detached.output);
    assert!(detached.output["agent_continuation_ref"].is_null());
    let after_detach = present_ref(&runtime, Some(&alice), &second_ref);
    assert_eq!(after_detach.output["error_kind"], "endpoint_detached");
    let unattached = runtime.list_agent_identities(Some(&alice), Some(agent_id), None, None);
    assert!(unattached.output["agents"][0]["agent_continuation_ref"].is_null());
}

#[test]
fn agent_continuation_ref_survives_restart_for_the_same_principal() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("restart.db");
    let alice = auth_context(Some("alice"), false);
    let continuation_ref = {
        let (_db, runtime) = runtime_with_db(&path);
        let agent_id = create_agent(&runtime, Some(&alice), "restart-agent");
        let rotated = rotate(&runtime, Some(&alice), &agent_id, "rotate");
        rotated["agent_continuation_ref"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let db = Arc::new(Database::open(&path).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_communication_database(db);
    let presented = present_ref(&runtime, Some(&alice), &continuation_ref);
    assert!(presented.success, "{:?}", presented.output);
}

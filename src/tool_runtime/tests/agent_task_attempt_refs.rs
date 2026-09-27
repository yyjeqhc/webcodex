use super::super::communication::communication_principal;
use super::support::auth_context;
use crate::tool_runtime::{ToolCall, ToolRuntime};
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

fn create_task(
    runtime: &ToolRuntime,
    auth: Option<&crate::auth::AuthContext>,
    assignee: &str,
    key: &str,
) -> String {
    let result = runtime.create_agent_task(
        auth,
        "Durable work".to_string(),
        "Perform bounded durable work.".to_string(),
        Some(assignee.to_string()),
        None,
        None,
        None,
        key.to_string(),
    );
    assert!(result.success, "{:?}", result.output);
    result.output["task"]["summary"]["task_id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn start_attempt(
    runtime: &ToolRuntime,
    auth: Option<&crate::auth::AuthContext>,
    task_id: &str,
    assignee: &str,
    key: &str,
) -> serde_json::Value {
    let result = runtime.start_agent_task_attempt(
        auth,
        task_id.to_string(),
        assignee.to_string(),
        key.to_string(),
    );
    assert!(result.success, "{:?}", result.output);
    result.output
}

fn continue_ref(
    runtime: &ToolRuntime,
    auth: Option<&crate::auth::AuthContext>,
    attempt_ref: &str,
) -> crate::tool_runtime::ToolResult {
    runtime.start_agent_task_endpoint_continuation_with_selector(
        auth,
        Some(attempt_ref.to_string()),
        None,
        None,
        None,
        None,
        None,
    )
}

#[test]
fn agent_task_attempt_ref_continues_the_pinned_tuple_and_fails_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("attempt-refs.db");
    let (db, runtime) = runtime_with_db(&path);
    let alice = auth_context(Some("alice"), false);
    let alice_other_key = {
        let mut auth = alice.clone();
        auth.api_key_id = Some("key-alice-other".to_string());
        auth
    };
    let bob = auth_context(Some("bob"), false);
    let assignee = create_agent(&runtime, Some(&alice), "alice-agent");
    let task_id = create_task(&runtime, Some(&alice), &assignee, "alice-task");
    let started = start_attempt(&runtime, Some(&alice), &task_id, &assignee, "start-1");
    let attempt_ref = started["attempt_ref"].as_str().unwrap().to_string();
    let fence = started["attempt_fence"].as_str().unwrap().to_string();
    let attempt_id = started["attempt"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_string();
    let generation = started["attempt"]["attempt_controller_generation"]
        .as_i64()
        .unwrap();
    assert!(attempt_ref.starts_with("~ta"));
    assert!(fence.starts_with("wc_agent_task_fence_"));
    assert!(!attempt_ref.contains(&fence));

    let replay = start_attempt(&runtime, Some(&alice), &task_id, &assignee, "start-1");
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["attempt_ref"], attempt_ref);
    assert_eq!(replay["attempt_fence"], fence);

    let explicit = runtime.start_agent_task_endpoint_continuation(
        Some(&alice),
        task_id.clone(),
        attempt_id.clone(),
        assignee.clone(),
        fence.clone(),
        generation,
    );
    assert!(explicit.success, "{:?}", explicit.output);
    assert_eq!(explicit.output["execution"]["attempt_id"], attempt_id);
    assert!(!explicit.output.to_string().contains(&fence));

    let continued = continue_ref(&runtime, Some(&alice), &attempt_ref);
    assert!(continued.success, "{:?}", continued.output);
    assert_eq!(continued.output["replayed"], true);
    assert_eq!(continued.output["execution"]["task_id"], task_id);
    let other_key = continue_ref(&runtime, Some(&alice_other_key), &attempt_ref);
    assert!(
        other_key.success,
        "the ref is not an API-key credential: {:?}",
        other_key.output
    );

    let foreign = continue_ref(&runtime, Some(&bob), &attempt_ref);
    assert!(!foreign.success, "{:?}", foreign.output);
    assert_eq!(
        foreign.output["error_kind"],
        "unknown_agent_task_attempt_ref"
    );
    let foreign_text = foreign.output.to_string();
    assert!(!foreign_text.contains(&fence));
    assert!(!foreign_text.contains(&attempt_id));

    for malformed in ["~ta", "~ta0", "~ta01", "~ac1", "~p1", &fence] {
        let rejected = continue_ref(&runtime, Some(&alice), malformed);
        assert!(!rejected.success, "{malformed}: {:?}", rejected.output);
        assert_eq!(
            rejected.output["error_kind"], "invalid_agent_task_attempt_ref",
            "{malformed}"
        );
    }
    let partial = runtime.start_agent_task_endpoint_continuation_with_selector(
        Some(&alice),
        None,
        Some(task_id.clone()),
        None,
        None,
        None,
        None,
    );
    assert_eq!(
        partial.output["error_kind"],
        "incomplete_agent_task_attempt_selector"
    );
    let mixed = runtime.start_agent_task_endpoint_continuation_with_selector(
        Some(&alice),
        Some(attempt_ref.clone()),
        Some(task_id.clone()),
        None,
        None,
        None,
        None,
    );
    assert_eq!(
        mixed.output["error_kind"],
        "ambiguous_agent_task_attempt_selector"
    );
    assert!(ToolCall::from_tool_name(
        "start_agent_task_endpoint_continuation",
        json!({"attempt_ref": attempt_ref, "session_id": "wc_sess_0123456789abcdef0123456789abcdef"}),
    )
    .is_err());

    let index: u64 = attempt_ref.strip_prefix("~ta").unwrap().parse().unwrap();
    let alice_principal = communication_principal(Some(&alice)).unwrap();
    assert_eq!(
        db.lookup_agent_task_attempt_reference(&alice_principal, index)
            .unwrap()
            .unwrap()
            .attempt_fence,
        fence
    );
    drop(runtime);
    drop(db);

    let (reopened, runtime) = runtime_with_db(&path);
    let after_restart = continue_ref(&runtime, Some(&alice), &attempt_ref);
    assert!(after_restart.success, "{:?}", after_restart.output);
    assert_eq!(after_restart.output["replayed"], true);
    assert_eq!(
        reopened
            .lookup_agent_task_attempt_reference(&alice_principal, index)
            .unwrap()
            .unwrap()
            .attempt_id,
        attempt_id
    );
}

#[test]
fn agent_task_attempt_ref_does_not_retarget_after_generation_expiry_or_takeover() {
    let tmp = tempfile::tempdir().unwrap();
    let (db, runtime) = runtime_with_db(&tmp.path().join("stale-attempt-refs.db"));
    let alice = auth_context(Some("alice"), false);
    let first = create_agent(&runtime, Some(&alice), "first");
    let second = create_agent(&runtime, Some(&alice), "second");
    let generation_task = create_task(&runtime, Some(&alice), &first, "generation-task");
    let started = start_attempt(
        &runtime,
        Some(&alice),
        &generation_task,
        &first,
        "generation-start",
    );
    let attempt_ref = started["attempt_ref"].as_str().unwrap().to_string();
    let fence = started["attempt_fence"].as_str().unwrap().to_string();
    let attempt_id = started["attempt"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_string();
    db.conn_for_tests()
        .execute(
            "UPDATE wc_agent_task_attempts SET attempt_controller_generation = 2 WHERE attempt_id = ?1",
            [attempt_id.as_str()],
        )
        .unwrap();
    let replay_after_replacement = start_attempt(
        &runtime,
        Some(&alice),
        &generation_task,
        &first,
        "generation-start",
    );
    assert_eq!(replay_after_replacement["replayed"], true);
    assert_eq!(
        replay_after_replacement["attempt_ref"], attempt_ref,
        "an idempotent start replay must return the original selector even after its pinned generation becomes stale"
    );
    assert_eq!(
        replay_after_replacement["attempt"]["attempt_controller_generation"], 2,
        "the replayed Attempt snapshot may still report current mutable state"
    );
    let stale_generation = continue_ref(&runtime, Some(&alice), &attempt_ref);
    assert_eq!(
        stale_generation.output["error_kind"],
        "agent_task_attempt_stale"
    );
    let stale_tuple = runtime.start_agent_task_endpoint_continuation(
        Some(&alice),
        generation_task.clone(),
        attempt_id.clone(),
        first.clone(),
        fence.clone(),
        1,
    );
    assert_eq!(
        stale_generation.output["error_kind"],
        stale_tuple.output["error_kind"]
    );
    assert!(!stale_generation.output.to_string().contains(&fence));
    let current = runtime.start_agent_task_endpoint_continuation(
        Some(&alice),
        generation_task,
        attempt_id.clone(),
        first.clone(),
        fence.clone(),
        2,
    );
    assert!(current.success, "{:?}", current.output);
    let alice_principal = communication_principal(Some(&alice)).unwrap();
    let index: u64 = attempt_ref.strip_prefix("~ta").unwrap().parse().unwrap();
    let pinned = db
        .lookup_agent_task_attempt_reference(&alice_principal, index)
        .unwrap()
        .unwrap();
    assert_eq!(pinned.attempt_controller_generation, 1);
    assert_eq!(pinned.attempt_id, attempt_id);

    let takeover_task = create_task(&runtime, Some(&alice), &first, "takeover-task");
    let expiring = start_attempt(
        &runtime,
        Some(&alice),
        &takeover_task,
        &first,
        "expire-start",
    );
    let old_ref = expiring["attempt_ref"].as_str().unwrap().to_string();
    let old_attempt = expiring["attempt"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_string();
    let old_fence = expiring["attempt_fence"].as_str().unwrap().to_string();
    db.conn_for_tests()
        .execute(
            "UPDATE wc_agent_task_attempts SET lease_expires_at_unix_ms = 0 WHERE attempt_id = ?1",
            [old_attempt.as_str()],
        )
        .unwrap();
    let expired = continue_ref(&runtime, Some(&alice), &old_ref);
    assert_eq!(expired.output["error_kind"], "agent_task_attempt_stale");
    let assigned = runtime.assign_agent_task(Some(&alice), takeover_task.clone(), second.clone());
    assert!(assigned.success, "{:?}", assigned.output);
    let taken_over = start_attempt(
        &runtime,
        Some(&alice),
        &takeover_task,
        &second,
        "takeover-start",
    );
    let new_ref = taken_over["attempt_ref"].as_str().unwrap().to_string();
    let new_attempt = taken_over["attempt"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(new_ref, old_ref);
    assert_ne!(new_attempt, old_attempt);
    assert_ne!(taken_over["attempt_fence"], old_fence);
    let stale_attempt = continue_ref(&runtime, Some(&alice), &old_ref);
    assert_eq!(
        stale_attempt.output["error_kind"],
        "agent_task_attempt_stale"
    );
    assert!(!stale_attempt.output.to_string().contains(&new_attempt));
    assert!(!stale_attempt.output.to_string().contains(&old_fence));
    let old_index: u64 = old_ref.strip_prefix("~ta").unwrap().parse().unwrap();
    let old_pin = db
        .lookup_agent_task_attempt_reference(&alice_principal, old_index)
        .unwrap()
        .unwrap();
    assert_eq!(old_pin.attempt_id, old_attempt);
    assert_eq!(old_pin.assignee_agent_id, first);
    let continued = continue_ref(&runtime, Some(&alice), &new_ref);
    assert!(continued.success, "{:?}", continued.output);
    assert_eq!(continued.output["execution"]["attempt_id"], new_attempt);
}

fn heartbeat_ref(
    runtime: &ToolRuntime,
    auth: Option<&crate::auth::AuthContext>,
    attempt_ref: Option<&str>,
    task_id: Option<&str>,
    attempt_id: Option<&str>,
    assignee: Option<&str>,
    fence: Option<&str>,
    generation: Option<i64>,
    wake_id: Option<&str>,
    consume_token: Option<&str>,
) -> crate::tool_runtime::ToolResult {
    runtime.heartbeat_agent_task_attempt_with_selector(
        auth,
        attempt_ref.map(str::to_string),
        task_id.map(str::to_string),
        attempt_id.map(str::to_string),
        assignee.map(str::to_string),
        fence.map(str::to_string),
        generation,
        wake_id.map(str::to_string),
        consume_token.map(str::to_string),
    )
}

#[test]
fn read_and_list_reissue_the_live_attempt_ref_for_heartbeat_and_completion() {
    let tmp = tempfile::tempdir().unwrap();
    let (db, runtime) = runtime_with_db(&tmp.path().join("recover-attempt-refs.db"));
    let alice = auth_context(Some("alice"), false);
    let bob = auth_context(Some("bob"), false);
    let assignee = create_agent(&runtime, Some(&alice), "owner");
    let task_id = create_task(&runtime, Some(&alice), &assignee, "recover-task");
    let started = start_attempt(&runtime, Some(&alice), &task_id, &assignee, "recover-start");
    let attempt_ref = started["attempt_ref"].as_str().unwrap().to_string();
    let fence = started["attempt_fence"].as_str().unwrap().to_string();
    let attempt_id = started["attempt"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_string();
    let generation = started["attempt"]["attempt_controller_generation"]
        .as_i64()
        .unwrap();

    let read = runtime.read_agent_task(Some(&alice), task_id.clone());
    assert!(read.success, "{:?}", read.output);
    assert_eq!(
        read.output["task"]["summary"]["attempt_ref"], attempt_ref,
        "a live read reissues the same selector"
    );
    assert!(read.output["task"]["summary"]
        .get("attempt_fence")
        .is_none());
    assert!(read.output["task"]["summary"]["latest_attempt"]
        .get("attempt_fence")
        .is_none());
    let listed = runtime.list_agent_tasks(Some(&alice), Some(assignee.clone()), None, Some(10));
    assert!(listed.success, "{:?}", listed.output);
    let listed_task = listed.output["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["task_id"] == task_id)
        .unwrap();
    assert_eq!(listed_task["attempt_ref"], attempt_ref);
    assert!(listed_task.get("attempt_fence").is_none());

    let by_tuple = heartbeat_ref(
        &runtime,
        Some(&alice),
        None,
        Some(&task_id),
        Some(&attempt_id),
        Some(&assignee),
        Some(&fence),
        Some(generation),
        None,
        None,
    );
    assert!(by_tuple.success, "{:?}", by_tuple.output);
    assert_eq!(by_tuple.output["attempt"]["attempt_id"], attempt_id);
    assert!(by_tuple.output.get("attempt_fence").is_none());
    let by_ref = heartbeat_ref(
        &runtime,
        Some(&alice),
        Some(&attempt_ref),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );
    assert!(by_ref.success, "{:?}", by_ref.output);
    assert_eq!(by_ref.output["attempt"]["attempt_id"], attempt_id);

    let ambiguous = heartbeat_ref(
        &runtime,
        Some(&alice),
        Some(&attempt_ref),
        Some(&task_id),
        None,
        None,
        None,
        None,
        None,
        None,
    );
    assert_eq!(
        ambiguous.output["error_kind"],
        "ambiguous_agent_task_attempt_selector"
    );
    let partial = heartbeat_ref(
        &runtime,
        Some(&alice),
        None,
        Some(&task_id),
        None,
        None,
        None,
        None,
        None,
        None,
    );
    assert_eq!(
        partial.output["error_kind"],
        "incomplete_agent_task_attempt_selector"
    );
    let foreign = heartbeat_ref(
        &runtime,
        Some(&bob),
        Some(&attempt_ref),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );
    assert_eq!(
        foreign.output["error_kind"],
        "unknown_agent_task_attempt_ref"
    );
    assert!(!foreign.output.to_string().contains(&fence));

    let proof_by_ref = heartbeat_ref(
        &runtime,
        Some(&alice),
        Some(&attempt_ref),
        None,
        None,
        None,
        None,
        None,
        Some("wc_wake_VVVVVVVVVVVVVVVV"),
        Some("wc_wake_consume_ZmZmZmZmZmZmZmZmZmZmZg"),
    );
    let proof_by_tuple = heartbeat_ref(
        &runtime,
        Some(&alice),
        None,
        Some(&task_id),
        Some(&attempt_id),
        Some(&assignee),
        Some(&fence),
        Some(generation),
        Some("wc_wake_VVVVVVVVVVVVVVVV"),
        Some("wc_wake_consume_ZmZmZmZmZmZmZmZmZmZmZg"),
    );
    assert!(!proof_by_ref.success);
    assert_eq!(
        proof_by_ref.output["error_kind"], proof_by_tuple.output["error_kind"],
        "active-turn proof stays on the existing heartbeat path"
    );

    db.conn_for_tests()
        .execute(
            "UPDATE wc_agent_task_attempts SET attempt_controller_generation = 2 WHERE attempt_id = ?1",
            [attempt_id.as_str()],
        )
        .unwrap();
    let stale_generation = heartbeat_ref(
        &runtime,
        Some(&alice),
        Some(&attempt_ref),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );
    assert_eq!(
        stale_generation.output["error_kind"],
        "agent_task_attempt_stale"
    );
    let reread = runtime.read_agent_task(Some(&alice), task_id.clone());
    let current_ref = reread.output["task"]["summary"]["attempt_ref"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(current_ref, attempt_ref);
    let current = heartbeat_ref(
        &runtime,
        Some(&alice),
        Some(&current_ref),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );
    assert!(current.success, "{:?}", current.output);
    assert_eq!(
        current.output["attempt"]["attempt_controller_generation"],
        2
    );

    let expiring_task = create_task(&runtime, Some(&alice), &assignee, "expire-recover");
    let expiring = start_attempt(
        &runtime,
        Some(&alice),
        &expiring_task,
        &assignee,
        "expire-recover-start",
    );
    let expired_ref = expiring["attempt_ref"].as_str().unwrap().to_string();
    let expired_attempt = expiring["attempt"]["attempt_id"].as_str().unwrap();
    db.conn_for_tests()
        .execute(
            "UPDATE wc_agent_task_attempts SET lease_expires_at_unix_ms = 0 WHERE attempt_id = ?1",
            [expired_attempt],
        )
        .unwrap();
    let expired_read = runtime.read_agent_task(Some(&alice), expiring_task);
    assert!(expired_read.success, "{:?}", expired_read.output);
    assert!(expired_read.output["task"]["summary"]
        .get("attempt_ref")
        .is_none());
    assert!(!expired_read.output.to_string().contains("attempt_fence"));
    let expired = heartbeat_ref(
        &runtime,
        Some(&alice),
        Some(&expired_ref),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );
    assert_eq!(expired.output["error_kind"], "agent_task_attempt_stale");

    let terminal_task = create_task(&runtime, Some(&alice), &assignee, "complete-recover");
    let terminal = start_attempt(
        &runtime,
        Some(&alice),
        &terminal_task,
        &assignee,
        "complete-start",
    );
    let terminal_ref = terminal["attempt_ref"].as_str().unwrap().to_string();
    let by_ref = runtime.complete_agent_task_attempt_with_selector(
        Some(&alice),
        Some(terminal_ref.clone()),
        None,
        None,
        None,
        None,
        None,
        "succeeded".to_string(),
        None,
        None,
        "complete-by-ref".to_string(),
    );
    assert!(by_ref.success, "{:?}", by_ref.output);
    assert_eq!(by_ref.output["attempt"]["state"], "succeeded");
    assert!(by_ref.output.get("attempt_fence").is_none());
    let replay_by_ref = runtime.complete_agent_task_attempt_with_selector(
        Some(&alice),
        Some(terminal_ref),
        None,
        None,
        None,
        None,
        None,
        "succeeded".to_string(),
        None,
        None,
        "complete-by-ref".to_string(),
    );
    assert!(replay_by_ref.success, "{:?}", replay_by_ref.output);
    assert_eq!(replay_by_ref.output["replayed"], true);
    assert_eq!(replay_by_ref.output["state_changed"], false);
    assert_eq!(replay_by_ref.output["attempt"]["state"], "succeeded");
    let other_task = create_task(&runtime, Some(&alice), &assignee, "complete-tuple");
    let other = start_attempt(
        &runtime,
        Some(&alice),
        &other_task,
        &assignee,
        "complete-tuple-start",
    );
    let by_tuple = runtime.complete_agent_task_attempt_with_selector(
        Some(&alice),
        None,
        Some(other_task),
        Some(other["attempt"]["attempt_id"].as_str().unwrap().to_string()),
        Some(assignee),
        Some(other["attempt_fence"].as_str().unwrap().to_string()),
        other["attempt"]["attempt_controller_generation"].as_i64(),
        "failed".to_string(),
        None,
        None,
        "complete-by-tuple".to_string(),
    );
    assert!(by_tuple.success, "{:?}", by_tuple.output);
    assert_eq!(by_tuple.output["attempt"]["state"], "failed");
}

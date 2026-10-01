use crate::tool_runtime::{AgentWaitEventSelectorCall, AgentWaitModeCall, ToolRuntime};
use std::sync::Arc;

fn assert_sparse_wait(
    tool: &str,
    arguments: serde_json::Value,
    canonical: &super::super::ToolResult,
) {
    let call = super::super::ToolCall::from_tool_name(tool, arguments).unwrap();
    let mut model = super::super::ToolResult {
        success: canonical.success,
        output: canonical.output.clone(),
        error: canonical.error.clone(),
    };
    super::super::result_projection::ModelFacingProjectionPlan::capture(&call).project(&mut model);
    let durable = &canonical.output["agent_wait"];
    let wait = &model.output["agent_wait"];
    for key in [
        "wait_id",
        "goal_id",
        "state",
        "mode",
        "source_count",
        "match_count",
    ] {
        assert_eq!(wait[key], durable[key], "model must preserve {key}");
    }
    assert_eq!(model.output["replayed"], canonical.output["replayed"]);
    assert_eq!(
        model.output["state_changed"],
        canonical.output["state_changed"]
    );
    assert_eq!(
        model.output["agent_continuation"],
        canonical.output["agent_continuation"]
    );
    for key in [
        "target_agent_id",
        "revision",
        "created_at_unix_ms",
        "updated_at_unix_ms",
        "triggered_at_unix_ms",
        "resumed_at_unix_ms",
        "cancelled_at_unix_ms",
        "match_sequence",
    ] {
        assert!(
            durable.get(key).is_some(),
            "durable {key} must remain intact"
        );
        assert!(wait.get(key).is_none(), "model must not receive {key}");
    }
    assert_eq!(wait.as_object().unwrap().len(), 8);
    for (source, original) in wait["sources"]
        .as_array()
        .unwrap()
        .iter()
        .zip(durable["sources"].as_array().unwrap())
    {
        assert_eq!(source.as_object().unwrap().len(), 2);
        for key in ["kind", "task_id"] {
            assert_eq!(source[key], original[key]);
        }
    }
    for (matched, original) in wait["matches"]
        .as_array()
        .unwrap()
        .iter()
        .zip(durable["matches"].as_array().unwrap())
    {
        assert_eq!(matched.as_object().unwrap().len(), 3);
        for key in ["task_id", "task_attempt_id", "terminal_task_state"] {
            assert_eq!(matched[key], original[key]);
        }
    }
    let schema = super::super::registry::output_schema_for_tool(tool);
    super::super::startup_brief::validate_schema_instance_for_test(
        &serde_json::to_value(model).unwrap(),
        &schema,
    )
    .unwrap();
}

#[test]
fn all_wait_model_projection_keeps_every_registered_source_identity_bounded() {
    let mut result = super::super::ToolResult {
        success: true,
        output: serde_json::json!({
            "agent_wait": {
                "wait_id": "wc_agent_wait_ERERERERERERERER",
                "target_agent_id": "wc_dagent_iavN7wEjRWeJq83v",
                "state": "resumed",
                "mode": "all",
                "revision": 5,
                "created_at_unix_ms": 1,
                "updated_at_unix_ms": 5,
                "triggered_at_unix_ms": 4,
                "resumed_at_unix_ms": 5,
                "cancelled_at_unix_ms": null,
                "source_count": 2,
                "match_count": 2,
                "match_sequence": 2,
                "sources": [
                    {
                        "ordinal": 0,
                        "kind": "agent_task_terminal",
                        "task_id": "wc_agent_task_IiIiIiIiIiIiIiIi",
                        "private_extra": "PRIVATE source extra"
                    },
                    {
                        "ordinal": 1,
                        "kind": "agent_task_terminal",
                        "task_id": "wc_agent_task_7u7u7u7u7u7u7u7u"
                    }
                ],
                "matches": [
                    {
                        "sequence": 1,
                        "kind": "agent_task_terminal",
                        "task_id": "wc_agent_task_IiIiIiIiIiIiIiIi",
                        "task_attempt_id": "wc_agent_task_attempt_MzMzMzMzMzMzMzMz",
                        "terminal_task_state": "succeeded",
                        "occurred_at_unix_ms": 3,
                        "private_result": "PRIVATE result"
                    },
                    {
                        "sequence": 2,
                        "kind": "agent_task_terminal",
                        "task_id": "wc_agent_task_7u7u7u7u7u7u7u7u",
                        "task_attempt_id": "wc_agent_task_attempt_QqQqQqQqQqQqQqQq",
                        "terminal_task_state": "failed",
                        "occurred_at_unix_ms": 4
                    }
                ]
            }
        }),
        error: None,
    };
    super::super::agent_wait::agent_wait_model_projection(&mut result);
    let wait = &result.output["agent_wait"];
    assert_eq!(wait["state"], "resumed");
    assert_eq!(wait["mode"], "all");
    assert_eq!(wait["source_count"], 2);
    assert_eq!(wait["match_count"], 2);
    assert_eq!(wait["sources"].as_array().unwrap().len(), 2);
    assert_eq!(
        wait["sources"][1]["task_id"],
        "wc_agent_task_7u7u7u7u7u7u7u7u"
    );
    assert_eq!(wait["matches"].as_array().unwrap().len(), 2);
    assert_eq!(wait.as_object().unwrap().len(), 7);
    let serialized = wait.to_string();
    for private in [
        "target_agent_id",
        "revision",
        "match_sequence",
        "ordinal",
        "sequence",
        "occurred_at_unix_ms",
        "PRIVATE source extra",
        "PRIVATE result",
    ] {
        assert!(
            !serialized.contains(private),
            "model projection leaked {private}"
        );
    }
}

#[test]
fn any_resumed_projection_keeps_unmatched_source_identity_for_authoritative_reread() {
    let mut result = super::super::ToolResult {
        success: true,
        output: serde_json::json!({
            "agent_wait": {
                "wait_id": "wc_agent_wait_AAAAAAAAAAAAAAAA",
                "target_agent_id": "wc_dagent_BBBBBBBBBBBBBBBB",
                "state": "resumed",
                "mode": "any",
                "revision": 4,
                "created_at_unix_ms": 1,
                "updated_at_unix_ms": 4,
                "triggered_at_unix_ms": 2,
                "resumed_at_unix_ms": 4,
                "cancelled_at_unix_ms": null,
                "source_count": 2,
                "match_count": 1,
                "match_sequence": 1,
                "sources": [
                    {"ordinal":0,"kind":"agent_task_terminal","task_id":"wc_agent_task_CCCCCCCCCCCCCCCC"},
                    {"ordinal":1,"kind":"agent_task_terminal","task_id":"wc_agent_task_DDDDDDDDDDDDDDDD"}
                ],
                "matches": [{
                    "sequence":1,
                    "kind":"agent_task_terminal",
                    "task_id":"wc_agent_task_CCCCCCCCCCCCCCCC",
                    "task_attempt_id":"wc_agent_task_attempt_EEEEEEEEEEEEEEEE",
                    "terminal_task_state":"succeeded",
                    "occurred_at_unix_ms":2
                }]
            }
        }),
        error: None,
    };
    super::super::agent_wait::agent_wait_model_projection(&mut result);
    let wait = &result.output["agent_wait"];
    assert_eq!(wait["state"], "resumed");
    assert_eq!(wait["mode"], "any");
    assert_eq!(wait["source_count"], 2);
    assert_eq!(wait["match_count"], 1);
    assert_eq!(
        wait["sources"][1]["task_id"], "wc_agent_task_DDDDDDDDDDDDDDDD",
        "fresh ANY continuation must retain the unmatched registered source identity"
    );
    assert_eq!(wait["matches"].as_array().unwrap().len(), 1);
}

fn runtime_with_db() -> (tempfile::TempDir, Arc<crate::db::Database>, ToolRuntime) {
    let temp = tempfile::tempdir().unwrap();
    let db = Arc::new(crate::db::Database::open(&temp.path().join("agent-waits.db")).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_communication_database(db.clone());
    (temp, db, runtime)
}

fn create_agent(runtime: &ToolRuntime, handle: &str) -> String {
    let result = runtime.create_agent_identity(
        None,
        handle.to_string(),
        format!("{handle} display"),
        None,
        Vec::new(),
        format!("create-{handle}"),
    );
    assert!(result.success, "{:?}", result.output);
    result.output["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn wait_runtime_surface_returns_exact_wait_and_existing_continuation_projection() {
    let (_temp, _db, runtime) = runtime_with_db();
    let watcher = create_agent(&runtime, "wait-runtime-watcher");
    let worker = create_agent(&runtime, "wait-runtime-worker");
    let endpoint = runtime.attach_agent_endpoint(
        None,
        watcher.clone(),
        "ChatGPT".to_string(),
        Some("wait-runtime-view".to_string()),
        "wait-runtime-endpoint".to_string(),
    );
    assert!(endpoint.success, "{:?}", endpoint.output);
    let endpoint_id = endpoint.output["endpoint"]["endpoint_id"]
        .as_str()
        .unwrap()
        .to_string();
    let generation = endpoint.output["endpoint"]["controller_generation"]
        .as_i64()
        .unwrap();

    let created_task = runtime.create_agent_task(
        None,
        "Wait runtime source".to_string(),
        "PRIVATE source instruction".to_string(),
        Some(worker.clone()),
        None,
        None,
        Some("agent:special:private-source-project".to_string()),
        "wait-runtime-task".to_string(),
    );
    assert!(created_task.success, "{:?}", created_task.output);
    let task_id = created_task.output["task"]["summary"]["task_id"]
        .as_str()
        .unwrap()
        .to_string();
    let started = runtime.start_agent_task_attempt(
        None,
        task_id.clone(),
        worker.clone(),
        "wait-runtime-attempt".to_string(),
    );
    assert!(started.success, "{:?}", started.output);
    let completed = runtime.complete_agent_task_attempt(
        None,
        task_id.clone(),
        started.output["attempt"]["attempt_id"]
            .as_str()
            .unwrap()
            .to_string(),
        worker.clone(),
        started.output["attempt_fence"]
            .as_str()
            .unwrap()
            .to_string(),
        started.output["attempt"]["attempt_controller_generation"]
            .as_i64()
            .unwrap(),
        "succeeded".to_string(),
        Some("PRIVATE terminal result".to_string()),
        Some("PRIVATE terminal reason".to_string()),
        "wait-runtime-complete".to_string(),
    );
    assert!(completed.success, "{:?}", completed.output);

    let unmatched_task = runtime.create_agent_task(
        None,
        "Wait runtime unmatched source".to_string(),
        "PRIVATE unmatched source instruction".to_string(),
        Some(worker),
        None,
        None,
        Some("agent:special:private-unmatched-project".to_string()),
        "wait-runtime-unmatched-task".to_string(),
    );
    assert!(unmatched_task.success, "{:?}", unmatched_task.output);
    let unmatched_task_id = unmatched_task.output["task"]["summary"]["task_id"]
        .as_str()
        .unwrap()
        .to_string();

    let waited = runtime.wait_for_agent_events(
        None,
        watcher.clone(),
        endpoint_id.clone(),
        generation,
        AgentWaitModeCall::Any,
        None,
        vec![
            AgentWaitEventSelectorCall {
                kind: "agent_task_terminal".to_string(),
                task_id: task_id.clone(),
            },
            AgentWaitEventSelectorCall {
                kind: "agent_task_terminal".to_string(),
                task_id: unmatched_task_id.clone(),
            },
        ],
        "wait-runtime-create".to_string(),
    );
    assert!(waited.success, "{:?}", waited.output);
    assert_eq!(waited.output["agent_wait"]["state"], "triggered");
    assert_eq!(waited.output["agent_wait"]["match_count"], 1);
    assert_eq!(waited.output["agent_wait"]["mode"], "any");
    assert_eq!(waited.output["agent_wait"]["source_count"], 2);
    assert_eq!(
        waited.output["agent_wait"]["sources"][1]["task_id"],
        unmatched_task_id
    );
    assert_eq!(
        waited.output["agent_wait"]["matches"][0]["task_id"],
        task_id
    );
    let wait_id = waited.output["agent_wait"]["wait_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        waited.output["agent_continuation"]["wake"]["wait_id"],
        wait_id
    );
    assert_eq!(
        waited.output["agent_continuation"]["wake"]["wait_match_count"],
        1
    );
    assert_sparse_wait(
        "wait_for_agent_events",
        serde_json::json!({
            "agent_id": watcher, "endpoint_id": endpoint_id, "expected_controller_generation": generation,
            "events": [
                {"kind": "agent_task_terminal", "task_id": task_id},
                {"kind": "agent_task_terminal", "task_id": unmatched_task_id}
            ],
            "idempotency_key": "wait-runtime-create"
        }),
        &waited,
    );
    let serialized = waited.output.to_string();
    for private in [
        "PRIVATE source instruction",
        "PRIVATE unmatched source instruction",
        "PRIVATE terminal result",
        "PRIVATE terminal reason",
        "private-source-project",
        "private-unmatched-project",
        "attempt_fence",
        "consume_token",
    ] {
        assert!(
            !serialized.contains(private),
            "Wait output leaked {private}"
        );
    }

    let read = runtime.read_agent_wait(None, wait_id.clone());
    assert!(read.success, "{:?}", read.output);
    assert_eq!(read.output["agent_wait"]["state"], "triggered");
    let app_read = runtime.agent_wait_state(None, wait_id.clone());
    assert!(app_read.success, "{:?}", app_read.output);
    assert_eq!(app_read.output, read.output);
    assert_sparse_wait(
        "read_agent_wait",
        serde_json::json!({"wait_id": wait_id}),
        &read,
    );

    let cancelled =
        runtime.cancel_agent_wait(None, wait_id.clone(), "wait-runtime-cancel".to_string());
    assert!(cancelled.success, "{:?}", cancelled.output);
    assert_eq!(cancelled.output["agent_wait"]["state"], "cancelled");
    assert_sparse_wait(
        "cancel_agent_wait",
        serde_json::json!({"wait_id": wait_id, "idempotency_key": "wait-runtime-cancel"}),
        &cancelled,
    );
    let replay = runtime.cancel_agent_wait(None, wait_id, "wait-runtime-cancel".to_string());
    assert!(replay.success, "{:?}", replay.output);
    assert_eq!(replay.output["replayed"], true);
    assert_eq!(replay.output["state_changed"], false);
}

#[test]
fn goal_scoped_wait_model_projection_exposes_only_exact_goal_reference() {
    let (_temp, _db, runtime) = runtime_with_db();
    let controller = create_agent(&runtime, "goal-scoped-projection-controller");
    let worker = create_agent(&runtime, "goal-scoped-projection-worker");
    let endpoint = runtime.attach_agent_endpoint(
        None,
        controller.clone(),
        "ChatGPT".to_string(),
        Some("goal-scoped-projection-view".to_string()),
        "goal-scoped-projection-endpoint".to_string(),
    );
    assert!(endpoint.success, "{:?}", endpoint.output);
    let endpoint_id = endpoint.output["endpoint"]["endpoint_id"]
        .as_str()
        .unwrap()
        .to_string();
    let generation = endpoint.output["endpoint"]["controller_generation"]
        .as_i64()
        .unwrap();

    let created_goal = runtime.create_goal_with_controller(
        None,
        "PRIVATE GOAL TITLE MUST NOT LEAK".to_string(),
        "PRIVATE GOAL OBJECTIVE MUST NOT LEAK".to_string(),
        Some(controller.clone()),
        "goal-scoped-projection-goal".to_string(),
    );
    assert!(created_goal.success, "{:?}", created_goal.output);
    let goal_id = created_goal.output["goal"]["summary"]["goal_id"]
        .as_str()
        .unwrap()
        .to_string();

    let created_task = runtime.create_agent_task(
        None,
        "Goal scoped projection source".to_string(),
        "PRIVATE TASK INSTRUCTION MUST NOT LEAK".to_string(),
        Some(worker),
        None,
        None,
        Some("agent:special:PRIVATE_PROJECT_MUST_NOT_LEAK".to_string()),
        "goal-scoped-projection-task".to_string(),
    );
    assert!(created_task.success, "{:?}", created_task.output);
    let task_id = created_task.output["task"]["summary"]["task_id"]
        .as_str()
        .unwrap()
        .to_string();
    let linked = runtime.associate_goal_agent_task(
        None,
        goal_id.clone(),
        task_id.clone(),
        "goal-scoped-projection-link".to_string(),
    );
    assert!(linked.success, "{:?}", linked.output);

    let waited = runtime.wait_for_agent_events(
        None,
        controller.clone(),
        endpoint_id.clone(),
        generation,
        AgentWaitModeCall::All,
        Some(goal_id.clone()),
        vec![AgentWaitEventSelectorCall {
            kind: "agent_task_terminal".to_string(),
            task_id: task_id.clone(),
        }],
        "goal-scoped-projection-wait".to_string(),
    );
    assert!(waited.success, "{:?}", waited.output);
    assert_eq!(waited.output["agent_wait"]["goal_id"], goal_id);
    assert_eq!(waited.output["agent_wait"]["state"], "waiting");
    assert_sparse_wait(
        "wait_for_agent_events",
        serde_json::json!({
            "agent_id": controller,
            "endpoint_id": endpoint_id,
            "expected_controller_generation": generation,
            "mode": "all",
            "goal_id": goal_id,
            "events": [{"kind":"agent_task_terminal","task_id":task_id}],
            "idempotency_key": "goal-scoped-projection-wait"
        }),
        &waited,
    );
    let serialized = waited.output["agent_wait"].to_string();
    for private in [
        "PRIVATE GOAL TITLE MUST NOT LEAK",
        "PRIVATE GOAL OBJECTIVE MUST NOT LEAK",
        "PRIVATE TASK INSTRUCTION MUST NOT LEAK",
        "PRIVATE_PROJECT_MUST_NOT_LEAK",
        "controller_generation",
        "terminal_result",
        "terminal_reason",
        "attempt_fence",
        "consume_token",
    ] {
        assert!(
            !serialized.contains(private),
            "Goal-scoped Wait model output leaked {private}"
        );
    }
}

#[test]
fn wait_tool_contracts_are_definition_owned_and_hidden_state_stays_app_only() {
    let specs = crate::tool_runtime::registered_tool_specs();
    for name in [
        "wait_for_agent_events",
        "read_agent_wait",
        "cancel_agent_wait",
    ] {
        let spec = specs.iter().find(|spec| spec.name == name).unwrap();
        assert!(spec.description.contains("AgentWait") || spec.description.contains("Agent Wait"));
    }
    let agent_wait_surface: std::collections::BTreeSet<&str> = specs
        .iter()
        .map(|spec| spec.name.as_str())
        .filter(|name| {
            matches!(
                *name,
                "wait_for_agent_events" | "read_agent_wait" | "cancel_agent_wait"
            )
        })
        .collect();
    assert_eq!(
        agent_wait_surface,
        std::collections::BTreeSet::from([
            "cancel_agent_wait",
            "read_agent_wait",
            "wait_for_agent_events",
        ]),
        "bounded ANY/ALL must not expand the direct model-visible AgentWait surface"
    );
    assert!(
        specs.iter().all(|spec| spec.name != "get_agent_wait_state"),
        "App-only Wait polling must stay hidden from the ordinary model-visible registry"
    );
    for forbidden_alias in [
        "wait_for_all_agent_events",
        "join_agent_tasks",
        "barrier_agent_tasks",
    ] {
        assert!(
            specs.iter().all(|spec| spec.name != forbidden_alias),
            "{forbidden_alias} must not become a second AgentWait representation"
        );
    }
}

#[tokio::test]
async fn agent_wait_dispatch_is_sparse_while_app_and_durable_read_remain_complete() {
    use super::super::{ToolCall, ToolResult};
    use serde_json::json;
    let (_temp, _db, runtime) = runtime_with_db();
    let auth = super::support::auth_context(None, true);
    let created_agent = runtime.create_agent_identity(
        Some(&auth),
        "sparse-wait-agent".into(),
        "Sparse Wait".into(),
        None,
        Vec::new(),
        "sparse-agent".into(),
    );
    assert!(created_agent.success);
    let agent = created_agent.output["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string();
    let endpoint = runtime.attach_agent_endpoint(
        Some(&auth),
        agent.clone(),
        "test".into(),
        None,
        "sparse-endpoint".into(),
    );
    let task = runtime.create_agent_task(
        Some(&auth),
        "source".into(),
        "instruction".into(),
        Some(agent.clone()),
        None,
        None,
        None,
        "sparse-source".into(),
    );
    assert!(endpoint.success && task.success);
    let arguments = json!({
        "agent_id": agent,
        "endpoint_id": endpoint.output["endpoint"]["endpoint_id"],
        "expected_controller_generation": endpoint.output["endpoint"]["controller_generation"],
        "events": [{"kind": "agent_task_terminal", "task_id": task.output["task"]["summary"]["task_id"]}],
        "idempotency_key": "sparse-create"
    });
    let call = || ToolCall::from_tool_name("wait_for_agent_events", arguments.clone()).unwrap();
    let created = runtime.dispatch_with_auth(call(), Some(&auth)).await;
    assert!(created.success, "{:?}", created);
    let wait_id = created.output["agent_wait"]["wait_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(created.output["agent_wait"]["wait_id"], wait_id);
    assert_eq!(created.output["agent_wait"]["state"], "waiting");
    assert_eq!(created.output["agent_wait"]["mode"], "any");
    assert_eq!(created.output["agent_wait"]["source_count"], 1);
    assert_eq!(created.output["agent_wait"]["match_count"], 0);
    assert_eq!(
        created.output["agent_wait"]["sources"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(created.output["agent_wait"]["matches"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(created.output["replayed"], false);
    assert_eq!(created.output["state_changed"], true);
    let replay = runtime.dispatch_with_auth(call(), Some(&auth)).await;
    assert_eq!(replay.output["agent_wait"], created.output["agent_wait"]);
    assert_eq!(replay.output["replayed"], true);
    assert_eq!(replay.output["state_changed"], false);
    let durable = runtime.read_agent_wait(Some(&auth), wait_id.clone());
    assert_sparse_wait("read_agent_wait", json!({"wait_id": wait_id}), &durable);
    let app = runtime
        .dispatch_with_auth(
            ToolCall::AgentWaitState {
                wait_id: wait_id.clone(),
            },
            Some(&auth),
        )
        .await;
    assert_eq!(app.output, durable.output);
    assert_eq!(app.output["agent_wait"]["target_agent_id"], agent);
    assert_eq!(app.output["agent_wait"]["revision"], 1);
    let audit =
        super::super::tool_audit::session_log_result_for_tool("read_agent_wait", &durable.output);
    assert_eq!(audit["revision"], 1);
    assert_eq!(audit["match_count"], 0);
    let cancelled = runtime
        .dispatch_with_auth(
            ToolCall::CancelAgentWait {
                wait_id: wait_id.clone(),
                idempotency_key: "sparse-cancel".into(),
            },
            Some(&auth),
        )
        .await;
    assert!(cancelled.success);
    assert_eq!(cancelled.output["agent_wait"]["wait_id"], wait_id);
    assert_eq!(cancelled.output["agent_wait"]["state"], "cancelled");
    assert_eq!(cancelled.output["agent_wait"]["mode"], "any");
    assert_eq!(cancelled.output["agent_wait"]["source_count"], 1);
    assert_eq!(cancelled.output["agent_wait"]["match_count"], 0);
    assert_eq!(cancelled.output["state_changed"], true);
    let mut failure = ToolResult::err("unknown Wait");
    let before = serde_json::to_value(&failure).unwrap();
    super::super::agent_wait::agent_wait_model_projection(&mut failure);
    assert_eq!(serde_json::to_value(&failure).unwrap(), before);
}

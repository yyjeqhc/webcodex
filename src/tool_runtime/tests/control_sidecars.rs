//! Control wrapper regression through the canonical kernel and domain stores.
use super::support::*;
use crate::auth::AuthContext;
use crate::tool_runtime::communication::communication_principal;
use crate::tool_runtime::control_sidecar::{self, ControlExecution, ControlSidecars};
use crate::tool_runtime::kernel::*;
use crate::tool_runtime::{ToolCall, ToolResult, ToolRuntime};
use serde_json::{json, Value};
use std::sync::Arc;

struct Fixture {
    _temp: tempfile::TempDir,
    db: Arc<crate::db::Database>,
    runtime: ToolRuntime,
    auth: AuthContext,
    goal: String,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(crate::db::Database::open(&temp.path().join("control.db")).unwrap());
        let runtime = ToolRuntime::new_for_tests().with_communication_database(db.clone());
        let auth = auth_context(Some("control-owner"), true);
        let created = runtime.create_goal_with_plan(
            Some(&auth),
            crate::db::NewGoal {
                title: "Sidecar contract".into(),
                objective: "Keep canonical mutation truth".into(),
                completion_conditions: vec!["All focused regressions pass".into()],
                controller_agent_id: None,
                idempotency_key: "goal".into(),
                steps: ["implement", "tests"]
                    .map(|id| crate::db::NewGoalStep {
                        id: id.into(),
                        title: id.into(),
                    })
                    .to_vec(),
            },
        );
        assert!(created.success);
        let goal = created.output["goal"]["summary"]["goal_id"]
            .as_str()
            .unwrap()
            .to_string();
        Self {
            _temp: temp,
            db,
            runtime,
            auth,
            goal,
        }
    }
    fn progress(&self) -> Value {
        json!({"goal_id": self.goal, "expected_revision": 1, "idempotency_key": "implement-done",
            "completed_step_ids": ["implement"], "current_step_id": "tests", "summary": "Implementation complete; begin tests"})
    }
    fn completion(&self, revision: i64) -> Value {
        json!({"goal_id": self.goal, "expected_revision": revision, "idempotency_key": "goal-done"})
    }
    fn context(&self) -> ToolCallContext<'_> {
        ToolCallContext {
            transport: ToolTransport::Mcp,
            session_id: None,
            auth: Some(&self.auth),
            window: None,
            record_oauth_scope_denials: false,
            host_file_import_trust: HostFileImportTrust::Untrusted,
        }
    }
    async fn call(&self, tool: &str, arguments: Value, control: Option<Value>) -> ToolResult {
        call(&self.runtime, self.context(), tool, arguments, control).await
    }
    fn revision(&self) -> i64 {
        self.runtime
            .get_goal(Some(&self.auth), self.goal.clone())
            .output["goal"]["summary"]["revision"]
            .as_i64()
            .unwrap()
    }
    async fn session(&self) -> String {
        let result = self.call("start_session", json!({}), None).await;
        assert!(result.success, "{:?}", result.error);
        result.output["session_id"].as_str().unwrap().to_string()
    }
    async fn post_phase(&self, control: Value, main: ToolResult) -> ToolResult {
        let mut execution = ControlExecution::new(serde_json::from_value(control).unwrap());
        execution
            .validate("finish_coding_task", self.context(), capabilities(), false)
            .unwrap();
        execution.main_dispatched = true;
        execution
            .after(&self.runtime, "finish_coding_task", &main, self.context())
            .await;
        let mut outcome = ToolCallOutcome {
            success: main.success,
            result: Some(main),
            error_status: None,
            project: None,
            model_ergonomics: None,
            correlation: Default::default(),
        };
        execution.decorate(&mut outcome);
        outcome.result.unwrap()
    }
}

fn capabilities() -> ToolProtocolCapabilities {
    ToolProtocolCapabilities {
        control_sidecars: true,
        ..Default::default()
    }
}

async fn call(
    runtime: &ToolRuntime,
    context: ToolCallContext<'_>,
    tool: &str,
    arguments: Value,
    control: Option<Value>,
) -> ToolResult {
    let outcome = runtime
        .call_tool_with_invocation_metadata(
            ToolCallRequest {
                tool_name: tool.into(),
                arguments,
            },
            context,
            ToolInvocationMetadata {
                control: control.map(|v| serde_json::from_value(v).unwrap()),
                ..Default::default()
            },
            capabilities(),
        )
        .await;
    assert!(outcome.error_status.is_none(), "{:?}", outcome.error_status);
    outcome.result.unwrap()
}

fn assert_not_started(result: &ToolResult) {
    assert!(!result.success);
    assert_eq!(
        result.output["control"]["main"]["execution_state"],
        "definitely_not_started"
    );
    assert_eq!(result.output["control"]["main"]["state_changed"], false);
}

#[test]
fn control_sidecars_closed_parse_strip_and_canonical_schema_parity() {
    let fixture = Fixture::new();
    let mut args = json!({"goal_id": fixture.goal, "_control": {"before": {"goal_progress": fixture.progress()}}});
    let parsed = control_sidecar::strip_control_sidecars(&mut args, "get_goal", true)
        .unwrap()
        .unwrap();
    assert!(ToolCall::from_tool_name("get_goal", args.clone()).is_ok());
    assert!(args.get("_control").is_none());
    assert!(parsed.before.is_some());
    assert_eq!(
        control_sidecar::strip_control_sidecars(&mut args, "get_goal", true).unwrap(),
        None
    );
    for value in [
        json!({"unknown": {}}),
        json!({"before": {"arbitrary_tool": {}}}),
        json!({"before": {"goal_progress": fixture.progress(), "wake_consume": {}}}),
        json!({"before": {"goal_progress": {"private": "secret"}}}),
        json!({"after_success": {"session_close": {"session_id": "s", "extra": true}}}),
        json!({"before": []}),
        json!({"before": {}}),
        json!({"communication": {}}),
        json!({"communication": {"before": [{"session_message": {"session_id": "wc_sess_abcdefghijklmnop", "kind": "progress", "message": "missing key"}}]}}),
        Value::Null,
    ] {
        assert!(serde_json::from_value::<ControlSidecars>(value).is_err());
    }
    for tool in [
        "goal_plan_sync",
        "agent_continuation_state",
        "plugin_tool",
        "ssh_resource",
    ] {
        assert!(
            control_sidecar::strip_control_sidecars(&mut json!({"_control": {}}), tool, true)
                .is_err()
        );
    }
    assert!(control_sidecar::strip_control_sidecars(
        &mut json!({"_control": {}}),
        "get_goal",
        false
    )
    .is_err());
    let schema = control_sidecar::input_schema("finish_coding_task");
    for (phase, kind, canonical) in [
        ("before", "goal_progress", "checkpoint_goal"),
        ("before", "wake_consume", "consume_agent_wake"),
        (
            "before",
            "attempt_heartbeat",
            "heartbeat_agent_task_attempt",
        ),
        ("before", "session_context_update", "update_session_context"),
        ("after_success", "session_close", "close_session"),
        (
            "after_success",
            "todo_completion",
            "complete_session_message",
        ),
    ] {
        assert_eq!(
            schema["properties"][phase]["properties"][kind],
            webcodex_tool_contracts::input_schema_for_tool(canonical)
        );
        assert_eq!(schema["properties"][phase]["maxProperties"], 1);
    }
    assert!(
        schema["properties"]["after_success"]["properties"]["goal_completion"]["properties"]
            .get("lifecycle")
            .is_none()
    );
    assert_eq!(
        schema["properties"]["communication"]["properties"]["before"]["maxItems"],
        2
    );
    assert!(
        schema["properties"]["communication"]["properties"]["before"]["items"]["properties"]
            ["session_message"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("delivery_key"))
    );
}

#[tokio::test]
async fn control_communication_session_message_replays_coexists_and_gates_after_success() {
    let f = Fixture::new();
    let session = f.session().await;
    let before_message = json!({"session_message": {
        "session_id": session,
        "kind": "progress",
        "message": "runtime review started",
        "tags": ["runtime"],
        "requires_ack": true,
        "delivery_key": "runtime-review-started"
    }});
    let control = json!({
        "before": {"goal_progress": f.progress()},
        "communication": {"before": [before_message]}
    });
    let result = f
        .call(
            "get_goal",
            json!({"goal_id": f.goal}),
            Some(control.clone()),
        )
        .await;
    assert!(result.success, "{:?}", result.output);
    assert_eq!(result.output["control"]["before"]["success"], true);
    assert_eq!(
        result.output["control"]["communication"]["before"][0]["success"],
        true
    );
    assert_eq!(
        result.output["control"]["communication"]["before"][0]["state_changed"],
        true
    );
    let message_id = result.output["control"]["communication"]["before"][0]["message_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        f.runtime
            .sessions
            .list_messages(&session, Default::default())
            .unwrap()
            .len(),
        1
    );

    let replay = f
        .call("get_goal", json!({"goal_id": f.goal}), Some(control))
        .await;
    assert!(replay.success);
    assert_eq!(
        replay.output["control"]["communication"]["before"][0]["message_id"],
        message_id
    );
    assert_eq!(
        replay.output["control"]["communication"]["before"][0]["replayed"],
        true
    );
    assert_eq!(
        f.runtime
            .sessions
            .list_messages(&session, Default::default())
            .unwrap()
            .len(),
        1
    );

    let standalone = f
        .call(
            "post_session_message",
            json!({
                "session_id": session,
                "kind": "progress",
                "message": "runtime review started",
                "tags": ["runtime"],
                "requires_ack": true,
                "delivery_key": "runtime-review-started"
            }),
            None,
        )
        .await;
    assert!(standalone.success);
    assert_eq!(standalone.output["message_id"], message_id);
    assert_eq!(standalone.output["replayed"], true);

    let after = json!({"communication": {"after_success": [{"session_message": {
        "session_id": session,
        "kind": "progress",
        "message": "main observation complete",
        "delivery_key": "main-observation-complete"
    }}]}});
    let failed = f
        .call(
            "get_goal",
            json!({"goal_id": "wc_goal_abcdefghijklmnop"}),
            Some(after.clone()),
        )
        .await;
    assert!(!failed.success);
    assert_eq!(
        failed.output["control"]["communication"]["after_success"][0]["execution_state"],
        "definitely_not_started"
    );
    assert_eq!(
        f.runtime
            .sessions
            .list_messages(&session, Default::default())
            .unwrap()
            .len(),
        1
    );
    let succeeded = f
        .call("get_goal", json!({"goal_id": f.goal}), Some(after))
        .await;
    assert!(succeeded.success);
    assert_eq!(
        succeeded.output["control"]["communication"]["after_success"][0]["success"],
        true
    );
    assert_eq!(
        f.runtime
            .sessions
            .list_messages(&session, Default::default())
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn control_communication_bounds_fail_closed_before_main() {
    let f = Fixture::new();
    let session = f.session().await;
    let message = |key: &str| {
        json!({"session_message": {
            "session_id": session,
            "kind": "note",
            "message": "bounded",
            "delivery_key": key
        }})
    };
    let result = f
        .call(
            "create_goal",
            json!({"title": "must not exist", "objective": "bound rejection", "idempotency_key": "bounded-main"}),
            Some(json!({"communication": {
                "before": [message("one"), message("two")],
                "after_success": [message("three")]
            }})),
        )
        .await;
    assert_not_started(&result);
    assert_eq!(
        result.output["error_kind"],
        "control_communication_limit_exceeded"
    );
    assert_eq!(
        f.runtime
            .sessions
            .list_messages(&session, Default::default())
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn control_sidecars_goal_progress_before_main_exact_replay_and_standalone_parity() {
    let f = Fixture::new();
    let control = json!({"before": {"goal_progress": f.progress()}});
    let result = f
        .call(
            "get_goal",
            json!({"goal_id": f.goal}),
            Some(control.clone()),
        )
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(
        result.output["goal"]["summary"]["revision"], 2,
        "main must observe checkpoint"
    );
    assert_eq!(result.output["control"]["before"]["revision"], 2);
    assert_eq!(result.output["control"]["before"]["state_changed"], true);
    let replay = f
        .call("get_goal", json!({"goal_id": f.goal}), Some(control))
        .await;
    assert!(replay.success);
    assert_eq!(replay.output["control"]["before"]["replayed"], true);
    assert_eq!(replay.output["control"]["before"]["state_changed"], false);
    let standalone = f.call("checkpoint_goal", f.progress(), None).await;
    assert!(standalone.success);
    assert_eq!(standalone.output["replayed"], true);
    assert!(standalone.output.get("control").is_none());
    assert_eq!(f.revision(), 2);
}

#[tokio::test]
async fn control_sidecars_goal_rejections_prove_main_did_not_start() {
    let f = Fixture::new();
    for (field, value) in [
        ("expected_revision", json!(9)),
        ("goal_id", json!("wc_goal_abcdefghijklmnop")),
        ("completed_step_ids", json!(["unknown"])),
    ] {
        let mut progress = f.progress();
        progress[field] = value;
        let result = f.call("create_goal", json!({"title": "Main effect", "objective": "Created only after before succeeds", "idempotency_key": "main-effect"}), Some(json!({"before": {"goal_progress": progress}}))).await;
        assert_not_started(&result);
        assert_eq!(
            f.db.conn_for_tests()
                .query_row("SELECT COUNT(*) FROM wc_goals", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(f.revision(), 1);
    }
    let done = f.call("checkpoint_goal", json!({"goal_id": f.goal, "expected_revision": 1, "idempotency_key": "all", "completed_step_ids": ["implement", "tests"], "summary": "done"}), None).await;
    assert!(done.success);
    assert!(f.call("update_goal", json!({"goal_id": f.goal, "expected_revision": 2, "idempotency_key": "terminal", "lifecycle": "completed"}), None).await.success);
    let mut progress = f.progress();
    progress["expected_revision"] = json!(3);
    assert_not_started(&f.call("create_goal", json!({"title": "Main effect", "objective": "Created only after before succeeds", "idempotency_key": "main-effect"}), Some(json!({"before": {"goal_progress": progress}}))).await);
}

#[tokio::test]
async fn control_sidecars_scope_and_permission_are_independent_of_main() {
    let mut f = Fixture::new();
    let mut restricted = auth_context(Some("control-owner"), false);
    restricted.scopes = vec![crate::auth::SCOPE_RUNTIME_READ.into()];
    let result = call(
        &f.runtime,
        ToolCallContext {
            auth: Some(&restricted),
            ..f.context()
        },
        "runtime_status",
        json!({}),
        Some(json!({"before": {"goal_progress": f.progress()}})),
    )
    .await;
    assert_not_started(&result);
    assert_eq!(result.output["error_kind"], "control_insufficient_scope");
    assert_eq!(f.revision(), 1);
    f.runtime = f.runtime.clone().with_permission_evaluator(
        crate::tool_runtime::permissions::PermissionEvaluator::with_mode(
            crate::tool_runtime::permissions::AuthorityMode::Restricted,
        ),
    );
    let result = f
        .call(
            "get_goal",
            json!({"goal_id": f.goal}),
            Some(json!({"before": {"goal_progress": f.progress()}})),
        )
        .await;
    assert_not_started(&result);
    assert_eq!(f.revision(), 1);
}

#[tokio::test]
async fn control_sidecars_no_implicit_mutations_and_internal_capability_fail_closed() {
    let f = Fixture::new();
    let plain = f.call("get_goal", json!({"goal_id": f.goal}), None).await;
    assert!(plain.success);
    assert!(plain.output.get("control").is_none());
    assert_eq!(f.revision(), 1);
    for transport in [ToolTransport::Api, ToolTransport::Mcp] {
        let result = f
            .runtime
            .call_tool_with_invocation_metadata(
                ToolCallRequest {
                    tool_name: "get_goal".into(),
                    arguments: json!({"goal_id": f.goal}),
                },
                ToolCallContext {
                    transport,
                    ..f.context()
                },
                ToolInvocationMetadata {
                    control: Some(
                        serde_json::from_value(json!({"before": {"goal_progress": f.progress()}}))
                            .unwrap(),
                    ),
                    ..Default::default()
                },
                ToolProtocolCapabilities::default(),
            )
            .await
            .result
            .unwrap();
        assert_not_started(&result);
    }
    assert_eq!(f.revision(), 1);
}

#[tokio::test]
async fn control_sidecars_post_failure_preserves_successful_main_and_never_retries_it() {
    let f = Fixture::new();
    let session = f.session().await;
    let result = f.call("create_goal", json!({"title": "Main effect", "objective": "Created only after before succeeds", "idempotency_key": "main-effect"}), Some(json!({"after_success": {"todo_completion": {
        "session_id": session, "message_id": "wc_msg_abcdefghijklmnop", "answer": "done", "completion_key": "answer", "expected_assignment_fence": "wsa2_abcdefghijklmnopqrstuv"
    }}}))).await;
    assert!(
        result.success,
        "main success must survive post failure: {:?}",
        result.output
    );
    assert!(result.output["goal"]["summary"]["goal_id"].is_string());
    assert_eq!(
        f.db.conn_for_tests()
            .query_row("SELECT COUNT(*) FROM wc_goals", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        result.output["control"]["main"]["execution_state"],
        "succeeded"
    );
    assert_eq!(result.output["control"]["after_success"]["success"], false);
}

#[tokio::test]
async fn control_sidecars_closeout_gates_goal_completion_and_session_close() {
    let f = Fixture::new();
    let session = f.session().await;
    let goal_control = json!({"after_success": {"goal_completion": f.completion(2)}});
    let session_control = json!({"after_success": {"session_close": {"session_id": session}}});
    for control in [goal_control.clone(), session_control.clone()] {
        for main in [
            ToolResult::err("main failed"),
            ToolResult::ok(json!({"task_outcome": {"blocking": true}})),
            ToolResult::ok(json!({})),
        ] {
            let result = f.post_phase(control.clone(), main).await;
            assert_eq!(
                result.output["control"]["after_success"]["execution_state"],
                "definitely_not_started"
            );
            assert_eq!(f.revision(), 1);
        }
    }
    let stale = f
        .post_phase(
            goal_control.clone(),
            ToolResult::ok(json!({"task_outcome": {"blocking": false}})),
        )
        .await;
    assert!(stale.success);
    assert_eq!(stale.output["control"]["after_success"]["success"], false);
    assert_eq!(stale.output["control"]["after_success"]["revision"], 1);
    let incomplete = f
        .post_phase(
            json!({"after_success": {"goal_completion": f.completion(1)}}),
            ToolResult::ok(json!({"task_outcome": {"blocking": false}})),
        )
        .await;
    assert!(incomplete.success);
    assert_eq!(
        incomplete.output["control"]["after_success"]["success"],
        false
    );
    assert_eq!(f.revision(), 1);
    assert!(f.call("checkpoint_goal", json!({"goal_id": f.goal, "expected_revision": 1, "idempotency_key": "all", "completed_step_ids": ["implement", "tests"], "summary": "done"}), None).await.success);
    for control in [goal_control, session_control] {
        let result = f
            .post_phase(
                control.clone(),
                ToolResult::ok(json!({"task_outcome": {"blocking": false}})),
            )
            .await;
        assert_eq!(result.output["control"]["after_success"]["success"], true);
        assert_eq!(
            result.output["control"]["after_success"]["state_changed"],
            true
        );
        let replay = f
            .post_phase(
                control,
                ToolResult::ok(json!({"task_outcome": {"blocking": false}})),
            )
            .await;
        assert_eq!(replay.output["control"]["after_success"]["replayed"], true);
        assert_eq!(
            replay.output["control"]["after_success"]["state_changed"],
            false
        );
    }
    assert_eq!(f.revision(), 3);
    assert_eq!(f.runtime.sessions.active_session_count_for_test(None), 0);
}

#[tokio::test]
async fn control_sidecars_context_update_fails_closed_without_replay_contract() {
    let f = Fixture::new();
    let result = f.call("get_goal", json!({"goal_id": f.goal}), Some(json!({"before": {"session_context_update": {
        "session_id": "wc_sess_abcdefghijklmnop", "project": "agent:local:demo", "execution_context": {"default_cwd": "."}
    }}}))).await;
    assert_not_started(&result);
    assert_eq!(
        result.output["control"]["before"]["error_kind"],
        "session_context_replay_contract_required"
    );
}

#[tokio::test]
async fn control_sidecars_wake_exact_consume_replay_and_stale_proofs() {
    use crate::db::{NewAgentEndpoint, NewConversation, NewConversationMessage};
    let f = Fixture::new();
    let principal = communication_principal(Some(&f.auth)).unwrap();
    let agent = f.runtime.create_agent_identity(
        Some(&f.auth),
        "control-agent".into(),
        "Control Agent".into(),
        None,
        vec![],
        "agent".into(),
    );
    assert!(agent.success);
    let agent_id = agent.output["agent"]["agent_id"].as_str().unwrap();
    let endpoint =
        f.db.attach_agent_endpoint(
            &principal,
            NewAgentEndpoint {
                agent_id: agent_id.into(),
                host: "test-host".into(),
                client_attachment_id: Some("attachment".into()),
                wake_capable: false,
                idempotency_key: "endpoint".into(),
            },
        )
        .unwrap()
        .endpoint;
    let conversation =
        f.db.create_conversation(
            &principal,
            NewConversation {
                title: Some("Sidecar wake".into()),
                agent_ids: vec![agent_id.into()],
                idempotency_key: "conversation".into(),
            },
        )
        .unwrap()
        .conversation
        .conversation
        .conversation_id;
    f.db.post_conversation_message(
        &principal,
        NewConversationMessage {
            conversation_id: conversation,
            body: "Wake for tests".into(),
            author_agent_id: None,
            endpoint_id: None,
            expected_controller_generation: None,
            recipient_agent_ids: Some(vec![agent_id.into()]),
            reply_to: None,
            idempotency_key: Some("wake-message".into()),
            wake_reply_id: None,
            reply_operation_index: None,
        },
    )
    .unwrap();
    let wake: String =
        f.db.conn_for_tests()
            .query_row(
                "SELECT wake_id FROM wc_agent_wakes WHERE target_agent_id = ?1",
                [agent_id],
                |row| row.get(0),
            )
            .unwrap();
    let activation =
        f.db.accept_explicit_agent_wake_activation(
            &principal,
            agent_id,
            &endpoint.endpoint_id,
            endpoint.controller_generation,
            &wake,
            "activate",
        )
        .unwrap();
    let consume = json!({"agent_id": agent_id, "endpoint_id": endpoint.endpoint_id, "expected_controller_generation": endpoint.controller_generation,
        "wake_id": wake, "consume_token": activation.consume_token});
    for (field, value) in [
        (
            "expected_controller_generation",
            json!(endpoint.controller_generation + 1),
        ),
        (
            "consume_token",
            json!(format!("wc_wake_consume_{}A", "z".repeat(21))),
        ),
    ] {
        let mut wrong = consume.clone();
        wrong[field] = value;
        assert_not_started(&f.call("create_goal", json!({"title": "Main effect", "objective": "Created only after before succeeds", "idempotency_key": "main-effect"}), Some(json!({"before": {"wake_consume": wrong}}))).await);
        assert_eq!(
            f.db.conn_for_tests()
                .query_row("SELECT COUNT(*) FROM wc_goals", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    for replayed in [false, true] {
        let result = f.call("create_goal", json!({"title": "Main effect", "objective": "Created only after before succeeds", "idempotency_key": "main-effect"}), Some(json!({"before": {"wake_consume": consume}}))).await;
        assert!(result.success, "{:?}", result.output);
        assert_eq!(result.output["control"]["before"]["replayed"], replayed);
        assert!(!result
            .output
            .to_string()
            .contains(&activation.consume_token));
        assert_eq!(
            f.db.agent_wake(&wake).unwrap().unwrap().state,
            crate::db::AgentWakeState::Consumed
        );
    }
    let standalone = f.call("consume_agent_wake", consume, None).await;
    assert!(standalone.success);
    assert_eq!(standalone.output["already_consumed"], true);
}

#[tokio::test]
async fn control_sidecars_heartbeat_exact_lease_and_no_implicit_renewal() {
    let f = Fixture::new();
    let agent = f.runtime.create_agent_identity(
        Some(&f.auth),
        "heartbeat-agent".into(),
        "Heartbeat Agent".into(),
        None,
        vec![],
        "agent".into(),
    );
    let agent_id = agent.output["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string();
    let task = f.runtime.create_agent_task(
        Some(&f.auth),
        "Heartbeat task".into(),
        "Bounded test".into(),
        Some(agent_id.clone()),
        None,
        None,
        None,
        "task".into(),
    );
    assert!(task.success);
    let task_id = task.output["task"]["summary"]["task_id"]
        .as_str()
        .unwrap()
        .to_string();
    let started = f.runtime.start_agent_task_attempt(
        Some(&f.auth),
        task_id.clone(),
        agent_id.clone(),
        "attempt".into(),
    );
    assert!(started.success);
    let attempt_id = started.output["attempt"]["attempt_id"].as_str().unwrap();
    let heartbeat = json!({"task_id": task_id, "attempt_id": attempt_id, "assignee_agent_id": agent_id, "attempt_fence": started.output["attempt_fence"], "attempt_controller_generation": 1});
    let expiry = || -> i64 {
        f.db.conn_for_tests()
            .query_row(
                "SELECT lease_expires_at_unix_ms FROM wc_agent_task_attempts WHERE attempt_id = ?1",
                [attempt_id],
                |row| row.get(0),
            )
            .unwrap()
    };
    let shortened = chrono::Utc::now().timestamp_millis() + 30_000;
    f.db.conn_for_tests()
        .execute(
            "UPDATE wc_agent_task_attempts SET lease_expires_at_unix_ms = ?1 WHERE attempt_id = ?2",
            rusqlite::params![shortened, attempt_id],
        )
        .unwrap();
    assert!(
        f.call("read_agent_task", json!({"task_id": task_id}), None)
            .await
            .success
    );
    assert_eq!(expiry(), shortened, "ordinary traffic never renews lease");
    for (field, value) in [
        ("attempt_controller_generation", json!(2)),
        (
            "attempt_fence",
            json!(format!("wc_agent_task_fence_{}A", "z".repeat(21))),
        ),
    ] {
        let mut wrong = heartbeat.clone();
        wrong[field] = value;
        assert_not_started(&f.call("create_goal", json!({"title": "Main effect", "objective": "Created only after before succeeds", "idempotency_key": "main-effect"}), Some(json!({"before": {"attempt_heartbeat": wrong}}))).await);
        assert_eq!(expiry(), shortened);
        assert_eq!(
            f.db.conn_for_tests()
                .query_row("SELECT COUNT(*) FROM wc_goals", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    let result = f
        .call(
            "read_agent_task",
            json!({"task_id": task_id}),
            Some(json!({"before": {"attempt_heartbeat": heartbeat}})),
        )
        .await;
    assert!(result.success);
    assert!(expiry() > shortened);
    assert_eq!(result.output["control"]["before"]["success"], true);
    assert!(!result
        .output
        .to_string()
        .contains(started.output["attempt_fence"].as_str().unwrap()));
    assert!(
        f.call("heartbeat_agent_task_attempt", heartbeat.clone(), None)
            .await
            .success
    );
    f.db.conn_for_tests()
        .execute(
            "UPDATE wc_agent_task_attempts SET lease_expires_at_unix_ms = 1 WHERE attempt_id = ?1",
            [attempt_id],
        )
        .unwrap();
    assert_not_started(&f.call("create_goal", json!({"title": "Main effect", "objective": "Created only after before succeeds", "idempotency_key": "main-effect"}), Some(json!({"before": {"attempt_heartbeat": heartbeat}}))).await);
}

#[tokio::test]
async fn control_sidecars_todo_exact_fence_main_failure_stale_and_replay() {
    let f = Fixture::new();
    let session = f.session().await;
    let todo = f.call("post_session_message", json!({"session_id": session, "kind": "todo", "message": "Complete after the observation succeeds"}), None).await;
    assert!(todo.success);
    let message_id = todo.output["message"]["message_id"].as_str().unwrap();
    let assignment = f
        .call(
            "get_session_assignment",
            json!({"session_id": session, "message_id": message_id}),
            None,
        )
        .await;
    assert!(assignment.success);
    let payload = json!({"session_id": session, "message_id": message_id, "answer": "Exact prepared answer", "completion_key": "answer", "expected_assignment_fence": assignment.output["assignment_fence"]});
    let control = json!({"after_success": {"todo_completion": payload}});
    let failed = f
        .call(
            "get_goal",
            json!({"goal_id": "wc_goal_abcdefghijklmnop"}),
            Some(control.clone()),
        )
        .await;
    assert!(!failed.success);
    assert_eq!(
        failed.output["control"]["after_success"]["execution_state"],
        "definitely_not_started"
    );
    let changed = f.call("post_session_message", json!({"session_id": session, "kind": "guidance", "message": "New acceptance condition", "reply_to": message_id}), None).await;
    assert!(changed.success);
    let stale = f
        .call(
            "session_summary",
            json!({"session_id": session}),
            Some(control),
        )
        .await;
    assert!(stale.success);
    assert_eq!(stale.output["control"]["after_success"]["success"], false);
    assert_eq!(
        stale.output["control"]["after_success"]["error_kind"],
        "assignment_stale"
    );
    let fresh = f
        .call(
            "get_session_assignment",
            json!({"session_id": session, "message_id": message_id}),
            None,
        )
        .await;
    let mut payload = payload;
    payload["expected_assignment_fence"] = fresh.output["assignment_fence"].clone();
    let done = f
        .call(
            "session_summary",
            json!({"session_id": session}),
            Some(json!({"after_success": {"todo_completion": payload}})),
        )
        .await;
    assert!(done.success);
    assert_eq!(done.output["control"]["after_success"]["success"], true);
    assert_eq!(
        done.output["control"]["after_success"]["state_changed"],
        true
    );
    let replay = f.call("complete_session_message", payload, None).await;
    assert!(replay.success);
    assert_eq!(replay.output["replayed"], true);
    assert!(!done.output["control"]
        .to_string()
        .contains("Exact prepared answer"));
}

#[tokio::test]
async fn control_sidecars_no_post_on_handoff_or_outcome_unknown() {
    let f = Fixture::new();
    for main in [
        ToolResult::ok(
            json!({"execution_state": "running", "terminal": false, "task_outcome": {"blocking": false}}),
        ),
        ToolResult::err_with_output("lost reply", json!({"execution_state": "outcome_unknown"})),
    ] {
        let result = f
            .post_phase(
                json!({"after_success": {"goal_completion": f.completion(1)}}),
                main,
            )
            .await;
        assert_eq!(
            result.output["control"]["after_success"]["execution_state"],
            "definitely_not_started"
        );
        assert_eq!(f.revision(), 1);
    }
}

#[tokio::test]
async fn control_sidecars_real_finish_dispatch_nonblocking_and_blocking() {
    use crate::tool_runtime::sessions::{SessionCreateOptions, SessionGuards, SessionTransport};
    for blocking in [false, true] {
        let f = Fixture::new();
        let project =
            register_runner_project_at_path(&f.runtime, "control-finish", "demo", f._temp.path())
                .await;
        let owner =
            crate::tool_runtime::workflow_session_authority_fingerprint(Some(&f.auth)).unwrap();
        let session = f
            .runtime
            .sessions
            .start_session_with_options(
                SessionCreateOptions::new(
                    Some(project.clone()),
                    Some("Control closeout".into()),
                    crate::tool_runtime::SessionMode::Normal,
                    SessionGuards::default(),
                )
                .with_owner_authority_fingerprint(Some(owner)),
            )
            .unwrap()
            .session_id;
        if blocking {
            let event = f.runtime.sessions.record_tool_call_started(
                Some(&session),
                SessionTransport::Mcp,
                "cargo_test",
                &json!({"project": project}),
                crate::tool_runtime::sessions::session_tool_contract("cargo_test"),
            );
            f.runtime.sessions.record_tool_call_finished(
                event,
                false,
                &json!({"exit_code": 1, "failure_kind": "validation_failed"}),
                Some("test fixture failure"),
                None,
            );
        }
        let task = tokio::spawn({
            let runtime = f.runtime.clone();
            let auth = f.auth.clone();
            let session = session.clone();
            async move {
                call(&runtime, ToolCallContext { transport: ToolTransport::Mcp, session_id: None, auth: Some(&auth), window: None, record_oauth_scope_denials: false, host_file_import_trust: HostFileImportTrust::Untrusted }, "finish_coding_task", json!({
                    "project": project, "session_id": session, "summary_only": true, "include_diff": false,
                    "include_workspace": true, "include_hygiene": false, "include_handoff": false, "include_validation_summary": false
                }), Some(json!({"after_success": {"session_close": {"session_id": session}}}))).await
            }
        });
        let request = wait_for_patch_agent_request(&f.runtime, "control-finish").await;
        let stdout =
            crate::tool_runtime::framed_clean_show_changes_test_stdout("clean fixture", false);
        complete_patch_agent_request(
            &f.runtime,
            "control-finish",
            &request.request_id,
            0,
            &stdout,
            "",
        )
        .await;
        let result = tokio::time::timeout(std::time::Duration::from_secs(10), task)
            .await
            .unwrap()
            .unwrap();
        assert!(result.success, "{:?}", result.output);
        assert_eq!(result.output["task_outcome"]["blocking"], blocking);
        assert_eq!(
            result.output["control"]["after_success"]["success"],
            !blocking
        );
        assert_eq!(
            f.runtime
                .sessions
                .summary(&session, None)
                .unwrap()
                .lifecycle
                .allows_mutation(),
            blocking
        );
    }
}

#[tokio::test]
async fn control_sidecars_phase_collision_and_early_post_authority_rejection() {
    let f = Fixture::new();
    let metadata = ToolInvocationMetadata {
        control: Some(
            serde_json::from_value(json!({"before": {"goal_progress": f.progress()}})).unwrap(),
        ),
        session_message_resolution: Some(
            crate::tool_runtime::sessions::ToolCallSessionMessageResolution {
                message_id: "wc_msg_abcdefghijklmnop".into(),
                resolution: "handled".into(),
            },
        ),
        ..Default::default()
    };
    let result = f
        .runtime
        .call_tool_with_invocation_metadata(
            ToolCallRequest {
                tool_name: "get_goal".into(),
                arguments: json!({"goal_id": f.goal}),
            },
            f.context(),
            metadata,
            capabilities(),
        )
        .await
        .result
        .unwrap();
    assert_not_started(&result);
    assert_eq!(result.output["error_kind"], "multiple_before_mutations");
    let result = f.call("get_goal", json!({"goal_id": f.goal}), Some(json!({"before": {"goal_progress": f.progress()}, "after_success": {"goal_completion": f.completion(2)}}))).await;
    assert_not_started(&result);
    assert_eq!(
        result.output["error_kind"],
        "control_requires_finish_coding_task"
    );
    assert_eq!(f.revision(), 1);
    let mut scoped = auth_context(Some("control-owner"), false);
    scoped.scopes = vec![
        crate::auth::SCOPE_COMMUNICATION_READ.into(),
        crate::auth::SCOPE_COMMUNICATION_MANAGE.into(),
    ];
    let result = call(&f.runtime, ToolCallContext { auth: Some(&scoped), ..f.context() }, "get_goal", json!({"goal_id": f.goal}), Some(json!({
        "before": {"goal_progress": f.progress()}, "after_success": {"todo_completion": {
            "session_id": "wc_sess_abcdefghijklmnop", "message_id": "wc_msg_abcdefghijklmnop", "answer": "done", "completion_key": "answer", "expected_assignment_fence": "wsa2_abcdefghijklmnopqrstuv"
        }}
    }))).await;
    assert_not_started(&result);
    assert_eq!(result.output["error_kind"], "control_insufficient_scope");
    assert_eq!(
        f.revision(),
        1,
        "post authority must be checked before a before mutation"
    );
}

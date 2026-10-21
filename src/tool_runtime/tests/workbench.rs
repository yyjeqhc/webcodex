use super::support::*;
use crate::runner_protocol::{RunnerCapabilities, RunnerRegisterRequest, RunnerResultPayload};
use crate::tool_runtime::ToolRuntime;
use serde_json::{json, Value};
use std::sync::Arc;
use webcodex_core::coding_agent::{
    CodingAgentExecutionState, CodingAgentProvider, CodingAgentResponse,
    CodingAgentResponsePayload, CodingAgentRunInventory, CodingAgentRunSnapshot,
    CodingAgentRunState,
};

async fn register_runner_with_run_inventory(
    runtime: &ToolRuntime,
    client_id: &str,
    project_id: &str,
    root: &std::path::Path,
    run: Option<CodingAgentRunSnapshot>,
) -> String {
    runtime
        .runner_registry
        .register(RunnerRegisterRequest {
            process_started_at: Some(1_700_000_000),
            coding_agent_providers: Some(vec![CodingAgentProvider {
                provider_id: "codex".into(),
                provider_instance_id: "codex-instance".into(),
                name: "Codex".into(),
            }]),
            coding_agent_inventory: Some(CodingAgentRunInventory {
                runs: run.into_iter().collect(),
            }),
            display_name: Some("Workbench recovery runner".into()),
            owner: Some("workbench-owner".into()),
            ..crate::test_support::runner_registration(
                client_id,
                "inst",
                RunnerCapabilities {
                    coding_agent_runs: true,
                    ..Default::default()
                },
            )
        })
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        client_id,
        "inst",
        vec![super::support::registered_project(
            project_id,
            &root.to_string_lossy(),
        )],
    )
    .await;
    crate::tool_runtime::runner_project_runtime_id(client_id, project_id)
}

fn submission(project: &str, agent_id: &str, key: &str) -> Value {
    json!({
        "project": project,
        "title": "Workbench durable execution",
        "instruction": "Execute this exact Workbench instruction.",
        "execution": {"kind":"runner", "agent_id":agent_id, "provider_id":"codex"},
        "goal_id": null,
        "idempotency_key": key
    })
}

fn agent_task_count(db: &crate::db::Database) -> i64 {
    db.conn_for_tests()
        .query_row("SELECT COUNT(*) FROM wc_agent_tasks", [], |row| row.get(0))
        .unwrap()
}

#[tokio::test]
async fn workbench_runner_submission_is_durable_and_same_key_replay_never_starts_twice() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("workbench-execution.db");
    let db = Arc::new(crate::db::Database::open(&db_path).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_communication_database(db.clone());
    let auth = bootstrap_auth_context();
    let agent = runtime.create_agent_identity(
        Some(&auth),
        webcodex_tool_contracts::CreateAgentIdentityInput {
            handle: "workbench-runner-agent".into(),
            display_name: "Workbench Runner Agent".into(),
            description: None,
            specialty_labels: Vec::new(),
            idempotency_key: "workbench-runner-agent-create".into(),
        },
    );
    assert!(agent.success, "{:?}", agent.output);
    let agent_id = agent.output["agent"]["agent_id"].as_str().unwrap();
    let client_id = "workbench-execution-runner";
    let project = register_runner_with_run_inventory(
        &runtime,
        client_id,
        "workbench-execution-project",
        temp.path(),
        None,
    )
    .await;

    let args = submission(&project, agent_id, "workbench-submit-once");
    let mut submit = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let args = args.clone();
        async move { runtime.submit_workbench_work(args, Some(&auth), None).await }
    });
    let request = tokio::select! {
        result = &mut submit => panic!("submission returned before Runner dispatch: {:?}", result.unwrap().output),
        request = wait_for_runner_request_for_instance(&runtime, client_id, "inst") => request,
    };
    let start = match request.coding_agent.as_ref().unwrap() {
        webcodex_core::coding_agent::CodingAgentRequest::Start(start) => start.clone(),
        other => panic!("expected CodingAgent Start, got {other:?}"),
    };
    let now = chrono::Utc::now().timestamp();
    let running = CodingAgentRunSnapshot {
        run_id: start.run_id.clone(),
        intent_fingerprint: start.intent_fingerprint.clone(),
        authority_fingerprint: start.authority_fingerprint.clone(),
        runtime_project_id: start.runtime_project_id.clone(),
        provider_id: start.provider_id.clone(),
        provider_instance_id: start.provider_instance_id.clone(),
        state: CodingAgentRunState::Running,
        execution_state: CodingAgentExecutionState::Started,
        observation_revision: 1,
        created_at: now,
        updated_at: now,
        terminal: None,
    };
    runtime
        .runner_registry
        .complete(RunnerResultPayload {
            result: runner_result_request(client_id.into(), "inst".into(), request.request_id),
            command_execution_state: None,
            mcp_gateway: None,
            plugin_gateway: None,
            coding_agent: Some(CodingAgentResponse::success(
                CodingAgentResponsePayload::Start {
                    run: running.clone(),
                },
            )),
        })
        .await
        .unwrap();
    let first = submit.await.unwrap();
    assert!(first.success, "{:?}", first.output);
    assert_eq!(first.output["execution"]["run_id"], start.run_id);
    let first_task = first.output["task_id"].as_str().unwrap().to_owned();
    let first_attempt = first.output["attempt"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(agent_task_count(&db), 1);
    assert_eq!(
        db.conn_for_tests()
            .query_row("SELECT COUNT(*) FROM wc_agent_task_attempts", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.conn_for_tests()
            .query_row(
                "SELECT COUNT(*) FROM wc_agent_task_coding_runs",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );

    drop(runtime);
    drop(db);
    let reopened_db = Arc::new(crate::db::Database::open(&db_path).unwrap());
    let recovered_runtime =
        ToolRuntime::new_for_tests().with_communication_database(reopened_db.clone());
    let recovered_project = register_runner_with_run_inventory(
        &recovered_runtime,
        client_id,
        "workbench-execution-project",
        temp.path(),
        Some(running),
    )
    .await;
    assert_eq!(recovered_project, project);
    let replay = recovered_runtime
        .submit_workbench_work(args.clone(), Some(&auth), None)
        .await;
    assert!(replay.success, "{:?}", replay.output);
    assert_eq!(replay.output["task_id"], first_task);
    assert_eq!(replay.output["attempt"]["attempt_id"], first_attempt);
    assert_eq!(agent_task_count(&reopened_db), 1);
    assert_eq!(
        reopened_db
            .conn_for_tests()
            .query_row("SELECT COUNT(*) FROM wc_agent_task_attempts", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        reopened_db
            .conn_for_tests()
            .query_row(
                "SELECT COUNT(*) FROM wc_agent_task_coding_runs",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    assert!(
        probe_agent_request_for_instance(&recovered_runtime, client_id, "inst")
            .await
            .is_none(),
        "same-key recovery must not issue a second CodingAgent Start"
    );

    let mut project_read_only = auth_context(Some("workbench-owner"), false);
    project_read_only.scopes = vec![
        crate::auth::SCOPE_COMMUNICATION_READ.into(),
        crate::auth::SCOPE_COMMUNICATION_MANAGE.into(),
        crate::auth::SCOPE_CODING_AGENT_RUN.into(),
        crate::auth::SCOPE_PROJECT_READ.into(),
    ];
    let mut read_only_projection = json!({
        "project": project,
        "agents": {"agents": [{"agent_id": agent_id, "display_name": "Workbench Runner Agent"}]},
        "providers": [{"provider_id": "codex"}],
        "tasks": {"tasks": []},
        "capabilities": {}
    });
    recovered_runtime
        .enrich_workbench_execution(
            &mut read_only_projection,
            Some(&project),
            Some(&project_read_only),
            None,
        )
        .await;
    assert!(
        !read_only_projection["execution_options"]
            .as_array()
            .unwrap()
            .iter()
            .any(|option| option["kind"] == "runner"),
        "Project without write authority must not expose a Runner executor: {read_only_projection}"
    );

    let mut projection = json!({
        "project": project,
        "agents": {"agents": [{"agent_id": agent_id, "display_name": "Workbench Runner Agent"}]},
        "providers": [{"provider_id": "codex"}],
        "tasks": {"tasks": [{"task_id": first_task, "latest_attempt": {"attempt_id": first_attempt}}]},
        "capabilities": {}
    });
    recovered_runtime
        .enrich_workbench_execution(&mut projection, Some(&project), Some(&auth), None)
        .await;
    assert!(
        projection["execution_options"]
            .as_array()
            .unwrap()
            .iter()
            .any(|option| { option["kind"] == "runner" && option["provider_id"] == "codex" }),
        "authorized connected Project/Runner should expose its CodingAgent executor: {projection}"
    );
    assert_eq!(projection["tasks"]["tasks"][0]["workbench_source"], "form");
    let actions = &projection["tasks"]["tasks"][0]["workbench_actions"];
    assert!(
        actions.get("observe").is_some(),
        "Runner inventory projection should provide an exact observation action: {projection}"
    );
    assert!(
        actions.get("reconcile").is_some(),
        "durable run projection should provide reconciliation: {projection}"
    );
    assert!(
        probe_agent_request_for_instance(&recovered_runtime, client_id, "inst")
            .await
            .is_none()
    );

    for field in ["title", "instruction"] {
        let mut changed = args.clone();
        changed[field] = json!(format!("changed {}", field));
        let rejected = recovered_runtime
            .submit_workbench_work(changed, Some(&auth), None)
            .await;
        assert!(
            !rejected.success,
            "changed {field} must conflict: {:?}",
            rejected.output
        );
    }
    let mut changed_executor = args;
    changed_executor["execution"]["provider_id"] = json!("other-provider");
    let rejected = recovered_runtime
        .submit_workbench_work(changed_executor, Some(&auth), None)
        .await;
    assert!(
        !rejected.success,
        "changed executor must conflict: {:?}",
        rejected.output
    );
    assert_eq!(agent_task_count(&reopened_db), 1);
    assert_eq!(
        reopened_db
            .conn_for_tests()
            .query_row("SELECT COUNT(*) FROM wc_agent_task_attempts", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        reopened_db
            .conn_for_tests()
            .query_row(
                "SELECT COUNT(*) FROM wc_agent_task_coding_runs",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    recovered_runtime
        .runner_registry
        .reconcile_disconnect(client_id, "inst")
        .await;
    let mut offline = recovered_runtime
        .read_workbench_state(Some(project.clone()), None, None, None, Some(&auth), None)
        .await;
    assert!(offline.success);
    recovered_runtime
        .enrich_workbench_execution(&mut offline.output, Some(&project), Some(&auth), None)
        .await;
    assert!(offline.output["execution_options"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(offline.output["errors"]
        .as_object()
        .unwrap()
        .values()
        .any(|error| matches!(
            error["error_kind"].as_str(),
            Some("runner_unavailable" | "project_unavailable")
        )));
    // A restart without the original Run in inventory still offers reconciliation,
    // never a retry/start action for that original Task.
    let uncertain_runtime =
        ToolRuntime::new_for_tests().with_communication_database(reopened_db.clone());
    register_runner_with_run_inventory(
        &uncertain_runtime,
        client_id,
        "workbench-execution-project",
        temp.path(),
        None,
    )
    .await;
    let mut uncertain = json!({"project":project,"agents":{"agents":[]},"providers":[],"tasks":{"tasks":[{"task_id":first_task,"latest_attempt":{"attempt_id":first_attempt}}]},"capabilities":{},"errors":{}});
    uncertain_runtime
        .enrich_workbench_execution(&mut uncertain, Some(&project), Some(&auth), None)
        .await;
    let actions = &uncertain["tasks"]["tasks"][0]["workbench_actions"];
    assert!(actions.get("reconcile").is_some(), "{uncertain}");
    assert!(
        actions.get("cancel").is_none() && actions.get("observe").is_none(),
        "{uncertain}"
    );
    let reconciled = uncertain_runtime
        .reconcile_agent_task_coding_run(Some(&auth), first_task, first_attempt)
        .await;
    assert!(reconciled.success, "{reconciled:?}");
    assert_eq!(reconciled.output["execution_status"], "outcome_unknown");
    assert_eq!(reconciled.output["execution_recovery"], "reconcile");
    assert!(
        probe_agent_request_for_instance(&uncertain_runtime, client_id, "inst")
            .await
            .is_none()
    );
    assert_eq!(agent_task_count(&reopened_db), 1);
}

#[tokio::test]
async fn workbench_rejects_missing_project_and_stale_chat_route_before_creating_a_task() {
    let temp = tempfile::tempdir().unwrap();
    let db =
        Arc::new(crate::db::Database::open(&temp.path().join("workbench-rejections.db")).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_communication_database(db.clone());
    let auth = bootstrap_auth_context();
    let agent = runtime.create_agent_identity(
        Some(&auth),
        webcodex_tool_contracts::CreateAgentIdentityInput {
            handle: "workbench-reject-agent".into(),
            display_name: "Workbench Reject Agent".into(),
            description: None,
            specialty_labels: Vec::new(),
            idempotency_key: "workbench-reject-agent-create".into(),
        },
    );
    assert!(agent.success, "{:?}", agent.output);
    let agent_id = agent.output["agent"]["agent_id"].as_str().unwrap();
    let project = register_runner_project_at_path_with_coding_agents(
        &runtime,
        "workbench-rejection-runner",
        "workbench-rejection-project",
        temp.path(),
        Some(vec![webcodex_core::coding_agent::CodingAgentProvider {
            provider_id: "codex".into(),
            provider_instance_id: "codex-instance".into(),
            name: "Codex".into(),
        }]),
    )
    .await;

    let missing_project = runtime
        .submit_workbench_work(
            submission("agent:missing:project", agent_id, "missing-project-key"),
            Some(&auth),
            None,
        )
        .await;
    assert!(!missing_project.success);
    assert_eq!(missing_project.output["error_kind"], "project_unavailable");
    assert_eq!(agent_task_count(&db), 0);

    let mut stale_chat = submission(&project, agent_id, "stale-chat-key");
    stale_chat["execution"] = json!({
        "kind":"chat", "agent_id":agent_id, "endpoint_id":"endpoint-old",
        "expected_controller_generation":1
    });
    let window = crate::client_window::ClientWindow::for_test("workbench-stale-window");
    let rejected = runtime
        .submit_workbench_work(stale_chat, Some(&auth), Some(&window))
        .await;
    assert!(!rejected.success);
    assert_eq!(rejected.output["error_kind"], "workbench_chat_unavailable");
    assert_eq!(agent_task_count(&db), 0);

    let unauthorized = auth_context(Some("workbench-no-scopes"), false);
    let denied = runtime
        .submit_workbench_work(
            submission("agent:missing:project", agent_id, "unauthorized-key"),
            Some(&unauthorized),
            None,
        )
        .await;
    assert!(!denied.success);
    assert_eq!(denied.output["error_kind"], "workbench_execution_denied");
    assert_eq!(agent_task_count(&db), 0);
}

#[tokio::test]
async fn workbench_chat_executor_is_projected_and_usable_only_in_its_bound_window() {
    let temp = tempfile::tempdir().unwrap();
    let db = Arc::new(crate::db::Database::open(&temp.path().join("workbench-chat.db")).unwrap());
    let policy =
        crate::model_workflow::ModelWorkflowPolicy::from_values(None, Some("unattended")).unwrap();
    let runtime = ToolRuntime::new_for_tests()
        .with_model_workflow_policy(policy)
        .with_communication_database(db.clone());
    let auth = bootstrap_auth_context();
    let agent = runtime.create_agent_identity(
        Some(&auth),
        webcodex_tool_contracts::CreateAgentIdentityInput {
            handle: "workbench-chat-agent".into(),
            display_name: "Workbench Chat Agent".into(),
            description: None,
            specialty_labels: Vec::new(),
            idempotency_key: "workbench-chat-agent-create".into(),
        },
    );
    assert!(agent.success, "{:?}", agent.output);
    let agent_id = agent.output["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let attached = runtime.attach_agent_endpoint(
        Some(&auth),
        webcodex_tool_contracts::RotateAgentContinuationEndpointInput {
            agent_id: agent_id.clone(),
            host: "ChatGPT".into(),
            client_attachment_id: None,
            idempotency_key: "workbench-chat-endpoint".into(),
        },
    );
    assert!(attached.success, "{:?}", attached.output);
    let endpoint_id = attached.output["endpoint"]["endpoint_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let generation = attached.output["endpoint"]["controller_generation"]
        .as_i64()
        .unwrap();
    let window = crate::client_window::ClientWindow::for_test("workbench-chat-window");
    let bound = runtime.agent_continuation_bind_for_window(
        Some(&auth),
        Some(&window),
        agent_id.clone(),
        endpoint_id.clone(),
        generation,
        "wc_host_binding_AAAAAAAAAAAAAAAAAAAAAA".into(),
    );
    assert!(bound.success, "{:?}", bound.output);
    let project = register_runner_project_at_path(
        &runtime,
        "workbench-chat-project-runner",
        "workbench-chat-project",
        temp.path(),
    )
    .await;

    let mut projected = json!({
        "project": project,
        "agents":{"agents":[{"agent_id":agent_id,"display_name":"Workbench Chat Agent"}]},
        "providers":[], "tasks":{"tasks":[]}, "capabilities":{}
    });
    runtime
        .enrich_workbench_execution(&mut projected, Some(&project), Some(&auth), Some(&window))
        .await;
    assert_eq!(projected["execution_options"].as_array().unwrap().len(), 1);
    assert_eq!(projected["execution_options"][0]["kind"], "chat");
    assert_eq!(
        projected["execution_options"][0]["endpoint_id"],
        endpoint_id
    );
    assert_eq!(
        projected["execution_options"][0]["expected_controller_generation"],
        generation
    );

    let foreign_window =
        crate::client_window::ClientWindow::for_test("workbench-chat-foreign-window");
    let mut foreign_projection = json!({
        "project": project,
        "agents":{"agents":[{"agent_id":agent_id,"display_name":"Workbench Chat Agent"}]},
        "providers":[], "tasks":{"tasks":[]}, "capabilities":{}
    });
    runtime
        .enrich_workbench_execution(
            &mut foreign_projection,
            Some(&project),
            Some(&auth),
            Some(&foreign_window),
        )
        .await;
    assert!(foreign_projection["execution_options"]
        .as_array()
        .unwrap()
        .is_empty());

    let mut args = submission(&project, &agent_id, "workbench-chat-foreign-submit");
    args["execution"] = json!({
        "kind":"chat", "agent_id":agent_id, "endpoint_id":endpoint_id,
        "expected_controller_generation":generation
    });
    let foreign = runtime
        .submit_workbench_work(args.clone(), Some(&auth), Some(&foreign_window))
        .await;
    assert!(
        !foreign.success,
        "foreign Window must not inherit the live binding: {:?}",
        foreign.output
    );
    assert_eq!(foreign.output["error_kind"], "workbench_chat_unavailable");
    assert_eq!(agent_task_count(&db), 0);

    args["idempotency_key"] = json!("workbench-chat-exact-submit");
    let submitted = runtime
        .submit_workbench_work(args, Some(&auth), Some(&window))
        .await;
    assert!(submitted.success, "{:?}", submitted.output);
    assert!(
        submitted.output["execution"].is_object(),
        "expected the canonical endpoint execution result: {:?}",
        submitted.output
    );
    assert_eq!(agent_task_count(&db), 1);
    assert_eq!(
        db.conn_for_tests()
            .query_row("SELECT COUNT(*) FROM wc_agent_task_attempts", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.conn_for_tests()
            .query_row(
                "SELECT COUNT(*) FROM wc_agent_task_endpoint_executions",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    let pinned: (String, i64) = db
        .conn_for_tests()
        .query_row(
            "SELECT pinned_endpoint_id, pinned_endpoint_controller_generation
             FROM wc_agent_task_endpoint_executions",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(pinned, (endpoint_id, generation));
}

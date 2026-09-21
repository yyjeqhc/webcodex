use super::support::*;
use crate::auth::scopes::{
    COMMUNICATION_MANAGE_SCOPES, COMMUNICATION_READ_SCOPES, SCOPE_COMMUNICATION_MANAGE,
    SCOPE_COMMUNICATION_READ, SCOPE_PROJECT_READ, SCOPE_RUNTIME_READ, SCOPE_SESSION_COLLABORATE,
};
use crate::tool_runtime::communication::communication_principal;
use crate::tool_runtime::metadata::{
    ToolApprovalPolicy, ToolAuthorityPolicy, ToolEffect, ToolIdempotency, ToolRisk,
};
use crate::tool_runtime::tool_definition::{lookup_tool_definition, RunnerCapabilityRequirement};
use crate::tool_runtime::{registered_tool_specs, ToolCall, ToolRuntime};
use serde_json::json;
use std::sync::Arc;

#[path = "goal_workflow.rs"]
mod workflow;

fn runtime_with_goal_db() -> (tempfile::TempDir, Arc<crate::db::Database>, ToolRuntime) {
    let temp = tempfile::tempdir().unwrap();
    let db = Arc::new(crate::db::Database::open(&temp.path().join("goals.db")).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_communication_database(db.clone());
    (temp, db, runtime)
}

fn create_goal(
    runtime: &ToolRuntime,
    auth: Option<&crate::auth::AuthContext>,
    key: &str,
) -> String {
    let created = runtime.create_goal(
        auth,
        "Durable Goal Phase 1".to_string(),
        "Keep high-level durable intent authoritative and independent from execution domains."
            .to_string(),
        key.to_string(),
    );
    assert!(created.success, "{:?}", created.output);
    created.output["goal"]["summary"]["goal_id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn runtime_with_goal_activity_db() -> (tempfile::TempDir, Arc<crate::db::Database>, ToolRuntime) {
    let temp = tempfile::tempdir().unwrap();
    let db = Arc::new(crate::db::Database::open(&temp.path().join("goal-activity.db")).unwrap());
    let runtime = ToolRuntime::new_for_tests()
        .with_communication_database(db.clone())
        .with_window_activity_database(db.clone());
    (temp, db, runtime)
}

fn goal_activity_auth(username: &str) -> crate::auth::AuthContext {
    let mut auth = auth_context(Some(username), false);
    auth.scopes = vec![
        SCOPE_COMMUNICATION_READ.to_string(),
        SCOPE_COMMUNICATION_MANAGE.to_string(),
        SCOPE_SESSION_COLLABORATE.to_string(),
        SCOPE_RUNTIME_READ.to_string(),
        SCOPE_PROJECT_READ.to_string(),
    ];
    auth
}

async fn register_goal_activity_project(
    runtime: &ToolRuntime,
    client_id: &str,
    owner: &str,
    project_id: &str,
    root: &std::path::Path,
) -> String {
    let instance_id = format!("inst-{client_id}");
    runtime
        .runner_registry
        .register(crate::runner_protocol::RunnerRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: Some(4),
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: client_id.to_string(),
            runner_instance_id: instance_id.clone(),
            runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
            display_name: Some(format!("{owner} Goal activity runner")),
            owner: Some(owner.to_string()),
            hostname: Some(format!("{owner}-goal-activity-host")),
            host_context: None,
            capabilities: crate::test_support::current_runner_capabilities(
                crate::runner_protocol::RunnerCapabilities::default(),
            ),
            policy: None,
        })
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        client_id,
        &instance_id,
        vec![registered_project(project_id, &root.to_string_lossy())],
    )
    .await;
    crate::tool_runtime::runner_project_runtime_id(client_id, project_id)
}

fn start_goal_activity_session(
    runtime: &ToolRuntime,
    auth: &crate::auth::AuthContext,
    project: &str,
    title: &str,
) -> crate::tool_runtime::sessions::SessionSummary {
    let fingerprint =
        super::super::session_context::workflow_session_authority_fingerprint(Some(auth))
            .expect("stable Goal activity test authority");
    runtime
        .sessions
        .start_session_with_options(
            crate::tool_runtime::sessions::SessionCreateOptions::new(
                Some(project.to_string()),
                Some(title.to_string()),
                crate::tool_runtime::SessionMode::Normal,
                crate::tool_runtime::sessions::SessionGuards::default(),
            )
            .with_owner_authority_fingerprint(Some(fingerprint)),
        )
        .unwrap()
}

async fn link_goal_activity_session(
    runtime: &ToolRuntime,
    auth: &crate::auth::AuthContext,
    goal_id: &str,
    session_id: &str,
    key: &str,
) {
    let linked = runtime
        .associate_goal_workflow_session(
            Some(auth),
            goal_id.to_string(),
            session_id.to_string(),
            key.to_string(),
        )
        .await;
    assert!(linked.success, "{:?}", linked.output);
}

#[allow(clippy::too_many_arguments)]
fn record_goal_window_event(
    db: &Arc<crate::db::Database>,
    auth: &crate::auth::AuthContext,
    window_id: &str,
    project: &str,
    operation: &str,
    meaningful: bool,
    linked_session_id: Option<&str>,
    at_ms: i64,
) {
    let window = crate::client_window::ClientWindow::for_test(window_id);
    let (principal_kind, principal_id) =
        crate::tool_runtime::runtime_observation_principal(Some(auth)).unwrap();
    crate::action_audit_sessions::record_action_event(
        db,
        crate::action_audit_sessions::ActionAuditEventInput {
            explicit_session_id: None,
            session_title: None,
            endpoint: "/mcp".to_string(),
            action_name: "toolsCall".to_string(),
            operation: Some(operation.to_string()),
            project: Some(project.to_string()),
            principal_kind: None,
            principal_user_id: None,
            oauth_client_id: None,
            status: "success".to_string(),
            http_status: Some(200),
            started_at: at_ms / 1000,
            ended_at: at_ms / 1000,
            duration_ms: 1,
            error_summary: None,
            warning_summary: None,
            changed_files: Vec::new(),
            ids: json!({}),
            summary: json!({}),
            request_bytes: None,
            response_bytes: None,
            client_window_key: Some(window.key().to_string()),
            client_window_source: Some(window.source().to_string()),
            server_trace_id: Some(format!("goal-activity-{window_id}-{operation}-{at_ms}")),
            principal_correlation_kind: Some(principal_kind),
            principal_correlation_id: Some(principal_id),
            window_started_at_ms: Some(at_ms),
            window_ended_at_ms: Some(at_ms + 1),
            request_observed_at_ms: None,
            response_handed_at_ms: None,
            window_transition_kind: None,
            response_streaming: None,
            window_continuity_eligible: None,
            window_meaningful: meaningful,
            recorder_gap_session_id: None,
            workflow_links: linked_session_id
                .map(|session_id| {
                    vec![crate::action_audit_sessions::ActionAuditWorkflowLinkInput {
                        workflow_session_id: session_id.to_string(),
                        relation: crate::action_audit_sessions::WorkflowSessionRelation::Recording,
                        project: Some(project.to_string()),
                    }]
                })
                .unwrap_or_default(),
        },
    );
}

fn goal_activity<'a>(result: &'a crate::tool_runtime::ToolResult) -> &'a serde_json::Value {
    &result.output["goal_plan"]["activity"]
}

#[test]
fn goal_tools_are_control_only_and_never_declare_execution_authority() {
    for name in [
        "create_goal",
        "prepare_goal_workflow",
        "get_goal",
        "present_goal_plan",
        "list_goals",
        "update_goal",
        "associate_goal_agent_task",
        "associate_goal_workflow_session",
    ] {
        let definition = lookup_tool_definition(name).unwrap_or_else(|| panic!("missing {name}"));
        assert!(
            definition.model_spec.is_some(),
            "{name} must be model-visible"
        );
        assert_eq!(definition.category, "goal");
        assert_eq!(definition.metadata.provider_id, "control");
        assert!(!definition.metadata.requires_project, "{name}");
        assert_eq!(
            definition.runner_capability, None::<RunnerCapabilityRequirement>,
            "{name}"
        );
        assert!(!definition.metadata.shell_like, "{name}");
        assert!(!matches!(
            definition.metadata.risk,
            ToolRisk::ProjectWrite | ToolRisk::JobRun
        ));
    }

    for name in ["get_goal", "list_goals", "present_goal_plan"] {
        let definition = lookup_tool_definition(name).unwrap();
        assert_eq!(definition.metadata.effect, ToolEffect::Observe);
        assert_eq!(definition.metadata.risk, ToolRisk::Read);
        assert_eq!(definition.metadata.approval, ToolApprovalPolicy::None);
        assert_eq!(definition.metadata.idempotency, ToolIdempotency::PureRead);
        assert_eq!(
            definition.metadata.authority,
            ToolAuthorityPolicy::RequireAll(COMMUNICATION_READ_SCOPES)
        );
    }

    for name in ["create_goal", "update_goal", "associate_goal_agent_task"] {
        let definition = lookup_tool_definition(name).unwrap();
        assert_eq!(definition.metadata.effect, ToolEffect::Mutate);
        assert_eq!(definition.metadata.risk, ToolRisk::WorkflowManage);
        assert_eq!(definition.metadata.approval, ToolApprovalPolicy::Standard);
        assert_eq!(definition.metadata.idempotency, ToolIdempotency::Keyed);
        assert_eq!(
            definition.metadata.authority,
            ToolAuthorityPolicy::RequireAll(COMMUNICATION_MANAGE_SCOPES)
        );
    }

    let app_state = lookup_tool_definition("goal_plan_sync").unwrap();
    assert!(app_state.model_spec.is_none());
    assert_eq!(app_state.category, "goal");
    assert_eq!(app_state.metadata.provider_id, "control");
    assert!(!app_state.metadata.requires_project);
    assert_eq!(
        app_state.runner_capability,
        None::<RunnerCapabilityRequirement>
    );
    assert!(!app_state.metadata.shell_like);
    assert_eq!(app_state.metadata.effect, ToolEffect::Mutate);
    assert_eq!(app_state.metadata.risk, ToolRisk::WorkflowManage);
    assert_eq!(app_state.metadata.approval, ToolApprovalPolicy::Standard);
    assert_eq!(
        app_state.metadata.idempotency,
        ToolIdempotency::DesiredState
    );
    assert_eq!(
        app_state.metadata.authority,
        ToolAuthorityPolicy::RequireAll(&[
            SCOPE_COMMUNICATION_READ,
            SCOPE_COMMUNICATION_MANAGE,
            SCOPE_RUNTIME_READ,
            SCOPE_SESSION_COLLABORATE,
            SCOPE_PROJECT_READ,
        ])
    );

    for name in ["prepare_goal_workflow", "associate_goal_workflow_session"] {
        let session_goal = lookup_tool_definition(name).unwrap();
        assert_eq!(session_goal.metadata.effect, ToolEffect::Mutate, "{name}");
        assert_eq!(
            session_goal.metadata.risk,
            ToolRisk::WorkflowManage,
            "{name}"
        );
        assert_eq!(
            session_goal.metadata.approval,
            ToolApprovalPolicy::Standard,
            "{name}"
        );
        assert_eq!(
            session_goal.metadata.idempotency,
            ToolIdempotency::Keyed,
            "{name}"
        );
        assert_eq!(
            session_goal.metadata.authority,
            ToolAuthorityPolicy::RequireAll(&[
                SCOPE_COMMUNICATION_READ,
                SCOPE_COMMUNICATION_MANAGE,
                SCOPE_SESSION_COLLABORATE,
            ]),
            "{name}"
        );
    }
}

#[test]
fn goal_schemas_are_bounded_private_and_existing_coding_tools_do_not_accept_goal_id() {
    let specs = registered_tool_specs();
    let spec = |name: &str| spec_named(&specs, name);

    let create = spec("create_goal");
    assert_eq!(create.input_schema["properties"]["title"]["maxLength"], 200);
    assert_eq!(
        create.input_schema["properties"]["objective"]["maxLength"],
        8192
    );
    assert_eq!(
        create.input_schema["properties"]["idempotency_key"]["maxLength"],
        128
    );
    assert_eq!(
        create.input_schema["properties"]["controller_agent_id"]["pattern"],
        "^wc_dagent_[A-Za-z0-9_-]{16}$"
    );
    assert!(!create.input_schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field == "controller_agent_id"));

    let prepare = spec("prepare_goal_workflow");
    assert_eq!(
        prepare.input_schema["properties"]["session_id"]["pattern"],
        "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"
    );
    assert_eq!(
        prepare.input_schema["properties"]["title"]["maxLength"],
        200
    );
    assert_eq!(
        prepare.input_schema["properties"]["objective"]["maxLength"],
        8192
    );
    assert_eq!(
        prepare.input_schema["properties"]["controller_agent_id"]["pattern"],
        "^wc_dagent_[A-Za-z0-9_-]{16}$"
    );
    assert_eq!(
        prepare.input_schema["properties"]["idempotency_key"]["maxLength"],
        128
    );
    let prepare_required = prepare.input_schema["required"].as_array().unwrap();
    for required in ["session_id", "title", "objective", "idempotency_key"] {
        assert!(
            prepare_required.iter().any(|field| field == required),
            "{required}"
        );
    }
    for forbidden in [
        "project",
        "client_window",
        "window",
        "endpoint_id",
        "expected_controller_generation",
        "binding_id",
        "host",
        "recording_session_id",
    ] {
        assert!(
            prepare.input_schema["properties"].get(forbidden).is_none(),
            "prepare_goal_workflow accepted Host/Project selector {forbidden}"
        );
    }
    let prepared_goal = &prepare.output_schema["properties"]["output"]["properties"]["goal"];
    assert!(prepared_goal["properties"].get("correlations").is_some());
    assert!(prepared_goal["properties"]
        .get("controller_agent_id")
        .is_some());
    let prepared_serialized = serde_json::to_string(prepared_goal).unwrap();
    for forbidden in [
        "production_auto_resume_available",
        "endpoint_id",
        "binding_id",
        "wake_id",
        "consume_token",
        "client_window",
        "project_path",
        "stdout",
        "stderr",
    ] {
        assert!(
            !prepared_serialized.contains(forbidden),
            "prepare_goal_workflow output schema leaked {forbidden}"
        );
    }

    let update = spec("update_goal");
    assert_eq!(
        update.input_schema["properties"]["lifecycle"]["enum"],
        json!(["active", "completed", "cancelled"])
    );
    assert_eq!(
        update.input_schema["properties"]["terminal_reason"]["maxLength"],
        4096
    );
    assert_eq!(
        update.input_schema["properties"]["controller_agent_id"]["pattern"],
        "^wc_dagent_[A-Za-z0-9_-]{16}$"
    );

    let present = spec("present_goal_plan");
    assert_eq!(present.input_schema["required"], json!(["goal_id"]));
    let plan = &present.output_schema["properties"]["output"]["properties"]["goal_plan"];
    assert_eq!(plan["properties"]["version"]["const"], 3);
    assert_eq!(
        plan["properties"]["activity"]["additionalProperties"],
        false
    );
    assert_eq!(
        plan["properties"]["activity"]["properties"]["idle_threshold_ms"]["const"],
        300_000
    );
    assert_eq!(
        plan["properties"]["activity"]["properties"]["observation_lease_ms"]["const"],
        75_000
    );
    assert_eq!(plan["properties"]["title"]["maxLength"], 200);
    assert!(plan["properties"].get("objective").is_none());
    assert_eq!(plan["properties"]["steps"]["maxItems"], 32);
    assert_eq!(
        plan["properties"]["controller_agent_id"]["anyOf"][0]["pattern"],
        "^wc_dagent_[A-Za-z0-9_-]{16}$"
    );
    assert_eq!(
        plan["properties"]["lifecycle"]["enum"],
        json!(["active", "completed", "cancelled"])
    );
    assert_eq!(plan["properties"]["agent_task_count"]["maximum"], 64);
    assert_eq!(plan["properties"]["workflow_session_count"]["maximum"], 64);
    let plan_properties = plan["properties"].as_object().unwrap();
    for forbidden_property in [
        "correlations",
        "terminal_reason",
        "attempt_fence",
        "authority_fingerprint",
        "consume_token",
        "wake_token",
        "session_ledger",
        "stdout",
        "stderr",
        "job_log",
    ] {
        assert!(
            !plan_properties.contains_key(forbidden_property),
            "Goal Plan schema exposed private property {forbidden_property}"
        );
    }
    let app_specs = crate::tool_runtime::goal_plan_app_tool_specs();
    assert_eq!(app_specs.len(), 1);
    assert_eq!(app_specs[0].name, "goal_plan_sync");
    for app_spec in &app_specs {
        assert_eq!(app_spec.input_schema["required"], json!(["goal_id"]));
        assert_eq!(
            app_spec.input_schema["properties"]
                .as_object()
                .unwrap()
                .len(),
            1
        );
        assert!(!registered_tool_names()
            .iter()
            .any(|name| name == &app_spec.name));
    }

    let list_summary =
        &spec("list_goals").output_schema["properties"]["output"]["properties"]["goals"]["items"];
    for field in [
        "goal_id",
        "title",
        "lifecycle",
        "revision",
        "created_at_unix_ms",
        "updated_at_unix_ms",
        "terminal_at_unix_ms",
        "agent_task_count",
        "workflow_session_count",
    ] {
        assert!(list_summary["properties"].get(field).is_some(), "{field}");
    }
    for private in [
        "objective",
        "controller_agent_id",
        "terminal_reason",
        "correlations",
    ] {
        assert!(
            list_summary["properties"].get(private).is_none(),
            "{private}"
        );
    }

    let detail = &spec("get_goal").output_schema["properties"]["output"]["properties"]["goal"];
    assert_eq!(
        detail["properties"]["controller_agent_id"]["anyOf"][0]["pattern"],
        "^wc_dagent_[A-Za-z0-9_-]{16}$"
    );
    let correlation = &detail["properties"]["correlations"]["items"];
    let mut correlation_fields = correlation["properties"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    correlation_fields.sort();
    assert_eq!(
        correlation_fields,
        vec![
            "created_at_unix_ms".to_string(),
            "kind".to_string(),
            "reference_id".to_string(),
        ]
    );
    let serialized_detail = serde_json::to_string(detail).unwrap();
    for forbidden in [
        "attempt_fence",
        "authority_fingerprint",
        "consume_token",
        "wake_token",
        "session_ledger",
        "stdout",
        "stderr",
    ] {
        assert!(
            !serialized_detail.contains(forbidden),
            "Goal detail schema leaked execution/private field {forbidden}"
        );
    }

    for existing in [
        "work_on_project",
        "read_files",
        "apply_text_edits",
        "run_process",
        "run_shell",
        "finish_coding_task",
    ] {
        let properties = spec(existing).input_schema["properties"]
            .as_object()
            .unwrap();
        assert!(
            !properties.contains_key("goal_id"),
            "{existing} must not gain Goal as an implicit selector or required execution context"
        );
    }
}

#[test]
fn goal_tool_calls_and_audit_keep_goal_identity_distinct_and_private_text_out_of_logs() {
    let goal_id = "wc_goal_AAAAAAAAAAAAAAAA".to_string();
    let call = ToolCall::from_tool_name(
        "update_goal",
        json!({
            "goal_id": goal_id,
            "expected_revision": 4,
            "objective": "PRIVATE_GOAL_OBJECTIVE_DO_NOT_LOG",
            "controller_agent_id": "wc_dagent_AAAAAAAAAAAAAAAA",
            "lifecycle": "completed",
            "terminal_reason": "PRIVATE_GOAL_REASON_DO_NOT_LOG",
            "idempotency_key": "PRIVATE_GOAL_KEY_DO_NOT_LOG"
        }),
    )
    .unwrap();
    assert_eq!(call.tool_name(), "update_goal");

    let audit = crate::tool_runtime::tool_audit::session_log_arguments_for_tool_request(
        "update_goal",
        &json!({
            "goal_id": goal_id,
            "expected_revision": 4,
            "objective": "PRIVATE_GOAL_OBJECTIVE_DO_NOT_LOG",
            "controller_agent_id": "wc_dagent_AAAAAAAAAAAAAAAA",
            "lifecycle": "completed",
            "terminal_reason": "PRIVATE_GOAL_REASON_DO_NOT_LOG",
            "idempotency_key": "PRIVATE_GOAL_KEY_DO_NOT_LOG"
        }),
    );
    assert_eq!(audit["goal_id"], goal_id);
    assert_eq!(audit["expected_revision"], 4);
    assert_eq!(audit["controller_agent_id"], "wc_dagent_AAAAAAAAAAAAAAAA");
    assert_eq!(
        audit["objective_bytes"],
        "PRIVATE_GOAL_OBJECTIVE_DO_NOT_LOG".len()
    );
    assert_eq!(
        audit["terminal_reason_bytes"],
        "PRIVATE_GOAL_REASON_DO_NOT_LOG".len()
    );
    assert_eq!(audit["idempotency_key_present"], true);
    let serialized = serde_json::to_string(&audit).unwrap();
    for private in [
        "PRIVATE_GOAL_OBJECTIVE_DO_NOT_LOG",
        "PRIVATE_GOAL_REASON_DO_NOT_LOG",
        "PRIVATE_GOAL_KEY_DO_NOT_LOG",
    ] {
        assert!(!serialized.contains(private), "Goal audit leaked {private}");
    }

    let prepare_session = "wc_sess_AAAAAAAAAAAAAAAA";
    let prepare_call = ToolCall::from_tool_name(
        "prepare_goal_workflow",
        json!({
            "session_id": prepare_session,
            "title": "PRIVATE_PREPARE_TITLE_DO_NOT_LOG",
            "objective": "PRIVATE_PREPARE_OBJECTIVE_DO_NOT_LOG",
            "completion_conditions": ["PRIVATE_PREPARE_CONDITION_DO_NOT_LOG"],
            "steps": [{"id": "implement", "title": "PRIVATE_PREPARE_STEP_DO_NOT_LOG"}],
            "controller_agent_id": "wc_dagent_AAAAAAAAAAAAAAAA",
            "idempotency_key": "PRIVATE_PREPARE_KEY_DO_NOT_LOG"
        }),
    )
    .unwrap();
    assert_eq!(prepare_call.tool_name(), "prepare_goal_workflow");
    assert_eq!(prepare_call.session_id(), None);
    let prepare_audit = crate::tool_runtime::tool_audit::session_log_arguments_for_tool_request(
        "prepare_goal_workflow",
        &json!({
            "session_id": prepare_session,
            "title": "PRIVATE_PREPARE_TITLE_DO_NOT_LOG",
            "objective": "PRIVATE_PREPARE_OBJECTIVE_DO_NOT_LOG",
            "completion_conditions": ["PRIVATE_PREPARE_CONDITION_DO_NOT_LOG"],
            "steps": [{"id": "implement", "title": "PRIVATE_PREPARE_STEP_DO_NOT_LOG"}],
            "controller_agent_id": "wc_dagent_AAAAAAAAAAAAAAAA",
            "idempotency_key": "PRIVATE_PREPARE_KEY_DO_NOT_LOG"
        }),
    );
    assert_eq!(prepare_audit["session_id"], prepare_session);
    assert_eq!(
        prepare_audit["controller_agent_id"],
        "wc_dagent_AAAAAAAAAAAAAAAA"
    );
    assert_eq!(prepare_audit["idempotency_key_present"], true);
    assert_eq!(
        prepare_audit["objective_bytes"],
        "PRIVATE_PREPARE_OBJECTIVE_DO_NOT_LOG".len()
    );
    let serialized_prepare_audit = serde_json::to_string(&prepare_audit).unwrap();
    for private in [
        "PRIVATE_PREPARE_TITLE_DO_NOT_LOG",
        "PRIVATE_PREPARE_OBJECTIVE_DO_NOT_LOG",
        "PRIVATE_PREPARE_CONDITION_DO_NOT_LOG",
        "PRIVATE_PREPARE_STEP_DO_NOT_LOG",
        "PRIVATE_PREPARE_KEY_DO_NOT_LOG",
    ] {
        assert!(
            !serialized_prepare_audit.contains(private),
            "prepare_goal_workflow audit leaked {private}"
        );
    }

    let plan_audit = crate::tool_runtime::tool_audit::session_log_arguments_for_tool_request(
        "present_goal_plan",
        &json!({"goal_id": goal_id}),
    );
    assert_eq!(plan_audit, json!({"goal_id": goal_id}));
    let private_objective = "PRIVATE_GOAL_PLAN_OBJECTIVE_DO_NOT_AUDIT";
    let result_audit = crate::tool_runtime::tool_audit::session_log_result_for_tool(
        "present_goal_plan",
        &json!({
            "goal_plan": {
                "goal_id": goal_id,
                "title": "Private title",
                "objective": private_objective,
                "lifecycle": "active",
                "revision": 7,
                "updated_at_unix_ms": 1,
                "terminal_at_unix_ms": null,
                "agent_task_count": 2,
                "workflow_session_count": 3
            }
        }),
    );
    assert_eq!(result_audit["goal_id"], goal_id);
    assert_eq!(result_audit["revision"], 7);
    assert_eq!(result_audit["agent_task_count"], 2);
    assert_eq!(result_audit["workflow_session_count"], 3);
    assert!(!serde_json::to_string(&result_audit)
        .unwrap()
        .contains(private_objective));

    assert!(ToolCall::from_tool_name(
        "associate_goal_agent_task",
        json!({
            "goal_id": "wc_goal_ERERERERERERERER".to_string(),
            "task_id": "wc_agent_task_IiIiIiIiIiIiIiIi".to_string(),
            "idempotency_key": "link"
        })
    )
    .is_ok());
}

#[test]
fn goal_runtime_crud_replay_and_exact_read_hide_foreign_existence() {
    let (_temp, _db, runtime) = runtime_with_goal_db();
    let bob = auth_context(Some("bob-goal"), false);
    let alice = auth_context(Some("alice-goal"), false);

    let first = runtime.create_goal(
        Some(&bob),
        "Private Bob Goal".to_string(),
        "Private objective".to_string(),
        "bob-create-goal".to_string(),
    );
    assert!(first.success, "{:?}", first.output);
    let goal_id = first.output["goal"]["summary"]["goal_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(first.output["goal"]["summary"]["revision"], 1);
    assert_eq!(first.output["goal"]["summary"]["lifecycle"], "active");

    let replay = runtime.create_goal(
        Some(&bob),
        "Private Bob Goal".to_string(),
        "Private objective".to_string(),
        "bob-create-goal".to_string(),
    );
    assert!(replay.success);
    assert_eq!(replay.output["replayed"], true);
    assert_eq!(replay.output["goal"]["summary"]["goal_id"], goal_id);

    let changed = runtime.create_goal(
        Some(&bob),
        "Private Bob Goal".to_string(),
        "Changed objective".to_string(),
        "bob-create-goal".to_string(),
    );
    assert!(!changed.success);
    assert_eq!(changed.output["error_kind"], "goal_idempotency_conflict");

    let foreign = runtime.get_goal(Some(&alice), goal_id.clone());
    let missing = runtime.get_goal(Some(&alice), "wc_goal_________________".to_string());
    assert!(!foreign.success);
    assert!(!missing.success);
    assert_eq!(foreign.output["error_kind"], "goal_not_found");
    assert_eq!(foreign.output["error_kind"], missing.output["error_kind"]);
    assert_eq!(foreign.error, missing.error);

    let updated = runtime.update_goal(
        Some(&bob),
        goal_id.clone(),
        1,
        Some("Updated Bob Goal".to_string()),
        None,
        None,
        None,
        "bob-update-goal".to_string(),
    );
    assert!(updated.success, "{:?}", updated.output);
    assert_eq!(updated.output["goal"]["summary"]["revision"], 2);

    let update_replay = runtime.update_goal(
        Some(&bob),
        goal_id,
        1,
        Some("Updated Bob Goal".to_string()),
        None,
        None,
        None,
        "bob-update-goal".to_string(),
    );
    assert!(update_replay.success);
    assert_eq!(update_replay.output["replayed"], true);
    assert_eq!(update_replay.output["goal"]["summary"]["revision"], 2);
}

#[tokio::test]
async fn prepare_goal_workflow_reauthorizes_session_project_controller_and_stays_host_neutral() {
    let (temp, db, runtime) = runtime_with_goal_activity_db();
    let owner = goal_activity_auth("prepare-goal-owner");
    let foreign = goal_activity_auth("prepare-goal-foreign");
    let project = register_goal_activity_project(
        &runtime,
        "prepare-goal-runner",
        "prepare-goal-owner",
        "demo",
        temp.path(),
    )
    .await;
    let session = start_goal_activity_session(&runtime, &owner, &project, "Prepare Goal Session");

    let controller = runtime.create_agent_identity(
        Some(&owner),
        "prepare-goal-controller".to_string(),
        "Prepare Goal Controller".to_string(),
        None,
        Vec::new(),
        "prepare-goal-controller-create".to_string(),
    );
    assert!(controller.success, "{:?}", controller.output);
    let controller_id = controller.output["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string();

    let input = |key: &str, controller_agent_id: Option<String>| crate::db::NewGoal {
        completion_conditions: vec!["Exact Session remains the initial correlation".to_string()],
        steps: vec![crate::db::NewGoalStep {
            id: "implement".to_string(),
            title: "Implement composition".to_string(),
        }],
        title: "Prepare Goal Workflow".to_string(),
        objective: "Atomically admit durable Goal workflow state without Host coupling."
            .to_string(),
        controller_agent_id,
        idempotency_key: key.to_string(),
    };

    let prepared = runtime
        .prepare_goal_workflow(
            Some(&owner),
            session.session_id.clone(),
            input("prepare-runtime", Some(controller_id.clone())),
        )
        .await;
    assert!(prepared.success, "{:?}", prepared.output);
    assert_eq!(prepared.output["created"], true);
    assert_eq!(prepared.output["replayed"], false);
    assert_eq!(prepared.output["state_changed"], true);
    assert_eq!(prepared.output["goal"]["summary"]["revision"], 1);
    assert_eq!(
        prepared.output["goal"]["summary"]["workflow_session_count"],
        1
    );
    assert_eq!(
        prepared.output["goal"]["controller_agent_id"],
        controller_id
    );
    assert_eq!(
        prepared.output["goal"]["correlations"],
        json!([{
            "kind": "workflow_session",
            "reference_id": session.session_id,
            "created_at_unix_ms": prepared.output["goal"]["summary"]["created_at_unix_ms"],
        }])
    );
    for host_specific in [
        "endpoint_id",
        "binding_id",
        "wake_id",
        "consume_token",
        "client_window",
        "production_auto_resume_available",
    ] {
        assert!(
            !prepared.output.to_string().contains(host_specific),
            "prepare output leaked Host-specific field {host_specific}"
        );
    }

    let replay = runtime
        .prepare_goal_workflow(
            Some(&owner),
            session.session_id.clone(),
            input("prepare-runtime", Some(controller_id)),
        )
        .await;
    assert!(replay.success, "{:?}", replay.output);
    assert_eq!(replay.output["created"], false);
    assert_eq!(replay.output["replayed"], true);
    assert_eq!(replay.output["state_changed"], false);
    assert_eq!(
        replay.output["goal"]["summary"]["goal_id"],
        prepared.output["goal"]["summary"]["goal_id"]
    );
    assert_eq!(replay.output["goal"]["summary"]["revision"], 1);

    let omitted = runtime
        .dispatch_with_auth(
            ToolCall::PrepareGoalWorkflow {
                session_id: session.session_id.clone(),
                completion_conditions: Vec::new(),
                steps: Vec::new(),
                title: "Prepare Goal Workflow via generic dispatcher".to_string(),
                objective: "Prove canonical ToolRuntime dispatch needs no MCP/App context."
                    .to_string(),
                controller_agent_id: None,
                idempotency_key: "prepare-runtime-no-controller".to_string(),
            },
            Some(&owner),
        )
        .await;
    assert!(omitted.success, "{:?}", omitted.output);
    assert!(omitted.output["goal"]["controller_agent_id"].is_null());
    assert_eq!(omitted.output["goal"]["summary"]["revision"], 1);

    let foreign_controller = runtime.create_agent_identity(
        Some(&foreign),
        "prepare-foreign-controller".to_string(),
        "Prepare Foreign Controller".to_string(),
        None,
        Vec::new(),
        "prepare-foreign-controller-create".to_string(),
    );
    assert!(
        foreign_controller.success,
        "{:?}",
        foreign_controller.output
    );
    let foreign_controller_id = foreign_controller.output["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string();
    let foreign_controller_error = runtime
        .prepare_goal_workflow(
            Some(&owner),
            session.session_id.clone(),
            input("prepare-foreign-controller", Some(foreign_controller_id)),
        )
        .await;
    let missing_controller_error = runtime
        .prepare_goal_workflow(
            Some(&owner),
            session.session_id.clone(),
            input(
                "prepare-missing-controller",
                Some("wc_dagent_________________".to_string()),
            ),
        )
        .await;
    assert!(!foreign_controller_error.success);
    assert!(!missing_controller_error.success);
    assert_eq!(
        foreign_controller_error.output["error_kind"],
        "agent_not_found"
    );
    assert_eq!(
        missing_controller_error.output["error_kind"],
        foreign_controller_error.output["error_kind"]
    );
    assert_eq!(
        missing_controller_error.error,
        foreign_controller_error.error
    );

    let owner_goal_count = db
        .list_goals(
            &communication_principal(Some(&owner)).unwrap(),
            None,
            0,
            100,
        )
        .unwrap()
        .total_count;
    let foreign_session_error = runtime
        .prepare_goal_workflow(
            Some(&foreign),
            session.session_id.clone(),
            input("prepare-foreign-session", None),
        )
        .await;
    assert!(!foreign_session_error.success);
    assert_eq!(
        db.list_goals(
            &communication_principal(Some(&owner)).unwrap(),
            None,
            0,
            100,
        )
        .unwrap()
        .total_count,
        owner_goal_count
    );

    let revoked_project = register_goal_activity_project(
        &runtime,
        "prepare-goal-runner",
        "different-project-owner",
        "demo",
        temp.path(),
    )
    .await;
    assert_eq!(revoked_project, project);
    assert!(runtime
        .resolve_project_input_for_auth(&project, Some(&owner))
        .await
        .is_err());
    let revoked = runtime
        .prepare_goal_workflow(
            Some(&owner),
            session.session_id.clone(),
            input("prepare-revoked-project", None),
        )
        .await;
    assert!(!revoked.success);
    assert_eq!(
        db.list_goals(
            &communication_principal(Some(&owner)).unwrap(),
            None,
            0,
            100,
        )
        .unwrap()
        .total_count,
        owner_goal_count
    );

    let conn = db.conn_for_tests();
    for table in [
        "wc_agent_endpoints",
        "wc_agent_wakes",
        "wc_agent_wake_attempts",
    ] {
        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            count, 0,
            "prepare_goal_workflow mutated Host carrier table {table}"
        );
    }
}

#[test]
fn goal_runtime_controller_is_explicit_authorized_and_readable() {
    let (_temp, _db, runtime) = runtime_with_goal_db();
    let bob = auth_context(Some("bob-goal-controller"), false);
    let alice = auth_context(Some("alice-goal-controller"), false);
    let controller = runtime.create_agent_identity(
        Some(&bob),
        "bob-goal-controller".to_string(),
        "Bob Goal Controller".to_string(),
        None,
        Vec::new(),
        "bob-goal-controller-agent".to_string(),
    );
    assert!(controller.success, "{:?}", controller.output);
    let controller_id = controller.output["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string();
    let replacement = runtime.create_agent_identity(
        Some(&bob),
        "bob-goal-controller-2".to_string(),
        "Bob Goal Controller 2".to_string(),
        None,
        Vec::new(),
        "bob-goal-controller-agent-2".to_string(),
    );
    assert!(replacement.success, "{:?}", replacement.output);
    let replacement_id = replacement.output["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string();
    let foreign = runtime.create_agent_identity(
        Some(&alice),
        "alice-goal-controller".to_string(),
        "Alice Goal Controller".to_string(),
        None,
        Vec::new(),
        "alice-goal-controller-agent".to_string(),
    );
    assert!(foreign.success, "{:?}", foreign.output);
    let foreign_id = foreign.output["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string();

    let created = runtime.create_goal_with_controller(
        Some(&bob),
        "Controller Goal".to_string(),
        "Use one explicit durable controller only for attention routing.".to_string(),
        Some(controller_id.clone()),
        "controller-goal-create".to_string(),
    );
    assert!(created.success, "{:?}", created.output);
    let goal_id = created.output["goal"]["summary"]["goal_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(created.output["goal"]["controller_agent_id"], controller_id);
    assert_eq!(
        runtime.get_goal(Some(&bob), goal_id.clone()).output["goal"]["controller_agent_id"],
        controller_id
    );

    let rejected = runtime.create_goal_with_controller(
        Some(&bob),
        "Foreign Controller Goal".to_string(),
        "Foreign durable Agent ids must remain existence-hidden.".to_string(),
        Some(foreign_id),
        "foreign-controller-goal-create".to_string(),
    );
    assert!(!rejected.success);
    assert_eq!(rejected.output["error_kind"], "agent_not_found");

    let updated = runtime.update_goal_with_controller(
        Some(&bob),
        goal_id,
        1,
        None,
        None,
        Some(replacement_id.clone()),
        None,
        None,
        "controller-goal-replace".to_string(),
    );
    assert!(updated.success, "{:?}", updated.output);
    assert_eq!(updated.output["goal"]["summary"]["revision"], 2);
    assert_eq!(
        updated.output["goal"]["controller_agent_id"],
        replacement_id
    );
}

#[tokio::test]
async fn goal_plan_projection_is_exact_pure_revisioned_terminal_and_existence_hidden() {
    let (_temp, _db, runtime) = runtime_with_goal_db();
    let bob = auth_context(Some("bob-plan"), false);
    let alice = auth_context(Some("alice-plan"), false);
    let controller = runtime.create_agent_identity(
        Some(&bob),
        "bob-plan-controller".to_string(),
        "Bob Plan Controller".to_string(),
        None,
        Vec::new(),
        "bob-plan-controller-agent".to_string(),
    );
    assert!(controller.success, "{:?}", controller.output);
    let controller_id = controller.output["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string();
    let created = runtime.create_goal_with_controller(
        Some(&bob),
        "Durable Goal Phase 1".to_string(),
        "Keep high-level durable intent authoritative and independent from execution domains."
            .to_string(),
        Some(controller_id.clone()),
        "bob-plan-create".to_string(),
    );
    assert!(created.success, "{:?}", created.output);
    let goal_id = created.output["goal"]["summary"]["goal_id"]
        .as_str()
        .unwrap()
        .to_string();

    let initial = runtime.present_goal_plan(Some(&bob), goal_id.clone()).await;
    assert!(initial.success, "{:?}", initial.output);
    let plan = &initial.output["goal_plan"];
    assert_eq!(plan["version"], 3);
    assert_eq!(plan["goal_id"], goal_id);
    assert_eq!(plan["lifecycle"], "active");
    assert_eq!(plan["revision"], 1);
    assert_eq!(plan["agent_task_count"], 0);
    assert_eq!(plan["workflow_session_count"], 0);
    assert_eq!(plan["controller_agent_id"], controller_id);
    assert_eq!(plan["activity"]["available"], false);
    assert_eq!(plan["activity"]["state"], "unobserved");
    assert!(plan["activity"]["last_seen_at_ms"].is_null());
    assert!(plan["activity"]["linked_window_count"].is_null());
    assert_eq!(plan["continuity"]["available"], false);
    assert_eq!(plan["continuity"]["state"], "unavailable");
    assert_eq!(
        plan["continuity"]["production_auto_resume_available"],
        false
    );
    assert!(plan["continuity"]["wake_state"].is_null());
    assert!(plan["terminal_at_unix_ms"].is_null());
    assert_eq!(
        plan.as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        [
            "activity",
            "agent_task_count",
            "controller_agent_id",
            "goal_id",
            "lifecycle",
            "total_step_count",
            "completed_step_count",
            "current_step_id",
            "steps",
            "progress_summary",
            "checkpoint_at_unix_ms",
            "continuity",
            "revision",
            "terminal_at_unix_ms",
            "title",
            "updated_at_unix_ms",
            "version",
            "workflow_session_count"
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    );

    let polled = runtime.present_goal_plan(Some(&bob), goal_id.clone()).await;
    assert!(polled.success);
    assert_eq!(polled.output["goal_plan"]["revision"], 1);
    assert_eq!(
        runtime.get_goal(Some(&bob), goal_id.clone()).output["goal"]["summary"]["revision"],
        1
    );

    let foreign = runtime
        .present_goal_plan(Some(&alice), goal_id.clone())
        .await;
    let missing = runtime
        .present_goal_plan(Some(&alice), "wc_goal_________________".to_string())
        .await;
    assert!(!foreign.success);
    assert!(!missing.success);
    assert_eq!(foreign.output["error_kind"], "goal_not_found");
    assert_eq!(foreign.error, missing.error);

    let updated = runtime.update_goal(
        Some(&bob),
        goal_id.clone(),
        1,
        None,
        Some("Updated durable plan objective".to_string()),
        None,
        None,
        "bob-plan-update".to_string(),
    );
    assert!(updated.success, "{:?}", updated.output);
    let after_update = runtime.present_goal_plan(Some(&bob), goal_id.clone()).await;
    assert!(after_update.success);
    assert_eq!(after_update.output["goal_plan"]["revision"], 2);
    assert_eq!(
        after_update.output["goal_plan"]["controller_agent_id"],
        controller_id
    );
    assert!(after_update.output["goal_plan"].get("objective").is_none());
    assert_eq!(
        runtime.get_goal(Some(&bob), goal_id.clone()).output["goal"]["objective"],
        "Updated durable plan objective"
    );

    let completed = runtime.update_goal(
        Some(&bob),
        goal_id.clone(),
        2,
        None,
        None,
        Some("completed".to_string()),
        Some("done".to_string()),
        "bob-plan-complete".to_string(),
    );
    assert!(completed.success, "{:?}", completed.output);
    let terminal = runtime.present_goal_plan(Some(&bob), goal_id.clone()).await;
    assert!(terminal.success);
    assert_eq!(terminal.output["goal_plan"]["lifecycle"], "completed");
    assert_eq!(terminal.output["goal_plan"]["revision"], 3);
    assert_eq!(
        terminal.output["goal_plan"]["controller_agent_id"],
        controller_id
    );
    assert_eq!(
        terminal.output["goal_plan"]["activity"]["state"],
        "not_applicable"
    );
    assert_eq!(terminal.output["goal_plan"]["activity"]["available"], false);
    assert_eq!(
        terminal.output["goal_plan"]["continuity"]["state"],
        "not_applicable"
    );
    assert_eq!(
        terminal.output["goal_plan"]["continuity"]["fresh_turn"],
        "not_applicable"
    );
    assert!(terminal.output["goal_plan"]["terminal_at_unix_ms"].is_i64());
    assert_eq!(
        runtime.get_goal(Some(&bob), goal_id).output["goal"]["summary"]["revision"],
        3
    );
}

#[tokio::test]
async fn goal_activity_tracks_window_wide_work_and_separates_seen_from_meaningful() {
    let (temp, db, runtime) = runtime_with_goal_activity_db();
    let auth = goal_activity_auth("goal-window-wide");
    let project_a = register_goal_activity_project(
        &runtime,
        "goal-live-a",
        "goal-window-wide",
        "a",
        temp.path(),
    )
    .await;
    let project_b = register_goal_activity_project(
        &runtime,
        "goal-live-b",
        "goal-window-wide",
        "b",
        temp.path(),
    )
    .await;
    let session = start_goal_activity_session(&runtime, &auth, &project_a, "Goal Session A");
    let goal_id = create_goal(&runtime, Some(&auth), "goal-window-wide-create");
    link_goal_activity_session(
        &runtime,
        &auth,
        &goal_id,
        &session.session_id,
        "goal-window-wide-link",
    )
    .await;

    let now = 10_000_000;
    let old = now - 7 * 60_000;
    record_goal_window_event(
        &db,
        &auth,
        "goal-window-wide",
        &project_a,
        "read_files",
        true,
        Some(&session.session_id),
        old,
    );
    let stale = runtime
        .goal_plan_sync_at(Some(&auth), goal_id.clone(), now)
        .await;
    assert!(stale.success, "{:?}", stale.output);
    assert_eq!(goal_activity(&stale)["state"], "attention_needed");
    assert_eq!(
        goal_activity(&stale)["last_meaningful_activity_at_ms"],
        old + 1
    );

    let poll_at = now - 3_000;
    record_goal_window_event(
        &db,
        &auth,
        "goal-window-wide",
        &project_a,
        "goal_plan_sync",
        false,
        None,
        poll_at,
    );
    let polled = runtime
        .goal_plan_sync_at(Some(&auth), goal_id.clone(), now)
        .await;
    assert_eq!(goal_activity(&polled)["state"], "attention_needed");
    assert_eq!(goal_activity(&polled)["last_seen_at_ms"], poll_at + 1);
    assert_eq!(
        goal_activity(&polled)["last_meaningful_activity_at_ms"],
        old + 1
    );

    // Historical Session A linkage discovers W, but later same-principal work in
    // another currently-visible Project/Session still refreshes Window-wide liveness.
    let recent = now - 60_000;
    record_goal_window_event(
        &db,
        &auth,
        "goal-window-wide",
        &project_b,
        "search_project_texts",
        true,
        None,
        recent,
    );
    let refreshed = runtime.goal_plan_sync_at(Some(&auth), goal_id, now).await;
    assert_eq!(goal_activity(&refreshed)["state"], "active");
    assert_eq!(
        goal_activity(&refreshed)["last_meaningful_activity_at_ms"],
        recent + 1
    );
    assert_eq!(goal_activity(&refreshed)["linked_window_count"], 1);
}

#[tokio::test]
async fn goal_activity_is_unobserved_without_windows_and_inflight_work_prevents_false_attention() {
    let (temp, db, runtime) = runtime_with_goal_activity_db();
    let auth = goal_activity_auth("goal-inflight");
    let project = register_goal_activity_project(
        &runtime,
        "goal-inflight",
        "goal-inflight",
        "demo",
        temp.path(),
    )
    .await;
    let session = start_goal_activity_session(&runtime, &auth, &project, "Goal inflight");
    let goal_id = create_goal(&runtime, Some(&auth), "goal-inflight-create");
    link_goal_activity_session(
        &runtime,
        &auth,
        &goal_id,
        &session.session_id,
        "goal-inflight-link",
    )
    .await;
    let now = 20_000_000;

    let unobserved = runtime
        .goal_plan_sync_at(Some(&auth), goal_id.clone(), now)
        .await;
    assert_eq!(goal_activity(&unobserved)["state"], "unobserved");
    assert_eq!(goal_activity(&unobserved)["linked_window_count"], 0);

    for (window, at) in [
        ("goal-inflight-old", now - 8 * 60_000),
        ("goal-inflight-recent", now - 2 * 60_000),
    ] {
        record_goal_window_event(
            &db,
            &auth,
            window,
            &project,
            "read_files",
            true,
            Some(&session.session_id),
            at,
        );
    }
    let multi = runtime
        .goal_plan_sync_at(Some(&auth), goal_id.clone(), now)
        .await;
    assert_eq!(goal_activity(&multi)["state"], "active");
    assert_eq!(goal_activity(&multi)["linked_window_count"], 2);

    let later = now + 10 * 60_000;
    let window = crate::client_window::ClientWindow::for_test("goal-inflight-old");
    let (kind, id) = crate::tool_runtime::runtime_observation_principal(Some(&auth)).unwrap();
    let guard = runtime.window_activity_registry().start_observed(
        &window,
        "goal-inflight-long-request",
        "tools/call",
        Some("run_process"),
        Some((&kind, &id)),
        later - 7 * 60_000,
    );
    guard.update(Some("run_process"), Some(&project));
    let running = runtime
        .goal_plan_sync_at(Some(&auth), goal_id.clone(), later)
        .await;
    assert_eq!(goal_activity(&running)["state"], "active");
    assert_eq!(
        goal_activity(&running)["active_meaningful_request_count"],
        1
    );

    let revision = runtime.get_goal(Some(&auth), goal_id.clone()).output["goal"]["summary"]
        ["revision"]
        .as_i64()
        .unwrap();
    let completed = runtime.update_goal(
        Some(&auth),
        goal_id.clone(),
        revision,
        None,
        None,
        Some("completed".to_string()),
        Some("terminal Goal ignores Window liveness".to_string()),
        "goal-inflight-terminal".to_string(),
    );
    assert!(completed.success, "{:?}", completed.output);
    let terminal = runtime.goal_plan_sync_at(Some(&auth), goal_id, later).await;
    assert_eq!(goal_activity(&terminal)["state"], "not_applicable");
    assert!(goal_activity(&terminal)["active_meaningful_request_count"].is_null());
    drop(guard);
}

#[tokio::test]
async fn goal_activity_respects_principal_project_visibility_and_runtime_read_scope() {
    let (temp, db, runtime) = runtime_with_goal_activity_db();
    let mut alice = goal_activity_auth("goal-visible-alice");
    alice.allowed_client_id = Some("goal-visible-client".to_string());
    let bob = goal_activity_auth("goal-visible-bob");
    let visible = register_goal_activity_project(
        &runtime,
        "goal-visible-client",
        "goal-visible-alice",
        "visible",
        temp.path(),
    )
    .await;
    let hidden = register_goal_activity_project(
        &runtime,
        "goal-hidden-client",
        "goal-visible-bob",
        "hidden",
        temp.path(),
    )
    .await;
    let session = start_goal_activity_session(&runtime, &alice, &visible, "Visible Goal Session");
    let goal_id = create_goal(&runtime, Some(&alice), "goal-visibility-create");
    link_goal_activity_session(
        &runtime,
        &alice,
        &goal_id,
        &session.session_id,
        "goal-visibility-link",
    )
    .await;
    let now = 30_000_000;
    let old = now - 8 * 60_000;
    record_goal_window_event(
        &db,
        &alice,
        "goal-visibility-window",
        &visible,
        "read_files",
        true,
        Some(&session.session_id),
        old,
    );
    record_goal_window_event(
        &db,
        &bob,
        "goal-visibility-window",
        &visible,
        "search_project_texts",
        true,
        None,
        now - 10_000,
    );
    record_goal_window_event(
        &db,
        &alice,
        "goal-visibility-window",
        &hidden,
        "run_process",
        true,
        None,
        now - 5_000,
    );
    let projected = runtime
        .goal_plan_sync_at(Some(&alice), goal_id.clone(), now)
        .await;
    assert_eq!(goal_activity(&projected)["state"], "unobserved");
    assert_eq!(goal_activity(&projected)["coverage_partial"], true);
    assert_eq!(goal_activity(&projected)["last_seen_at_ms"], old + 1);
    assert_eq!(
        goal_activity(&projected)["last_meaningful_activity_at_ms"],
        old + 1
    );

    let mut no_runtime_read = alice.clone();
    no_runtime_read
        .scopes
        .retain(|scope| scope != SCOPE_RUNTIME_READ);
    let unavailable = runtime
        .goal_plan_sync_at(Some(&no_runtime_read), goal_id, now)
        .await;
    assert!(unavailable.success, "{:?}", unavailable.output);
    let activity = goal_activity(&unavailable);
    assert_eq!(activity["available"], false);
    assert_eq!(activity["state"], "unobserved");
    for field in [
        "last_seen_at_ms",
        "last_meaningful_activity_at_ms",
        "quiet_for_ms",
        "linked_window_count",
        "active_meaningful_request_count",
    ] {
        assert!(activity[field].is_null(), "{field}");
    }
}

#[tokio::test]
async fn goal_activity_bounded_partial_window_scan_never_manufactures_attention() {
    let (temp, db, runtime) = runtime_with_goal_activity_db();
    let auth = goal_activity_auth("goal-partial");
    let project = register_goal_activity_project(
        &runtime,
        "goal-partial",
        "goal-partial",
        "demo",
        temp.path(),
    )
    .await;
    let session = start_goal_activity_session(&runtime, &auth, &project, "Partial Goal Session");
    let goal_id = create_goal(&runtime, Some(&auth), "goal-partial-create");
    link_goal_activity_session(
        &runtime,
        &auth,
        &goal_id,
        &session.session_id,
        "goal-partial-link",
    )
    .await;
    let now = 40_000_000;
    for index in 0..17 {
        record_goal_window_event(
            &db,
            &auth,
            &format!("goal-partial-window-{index}"),
            &project,
            "read_files",
            true,
            Some(&session.session_id),
            now - (10 + index as i64) * 60_000,
        );
    }
    let projection = runtime.goal_plan_sync_at(Some(&auth), goal_id, now).await;
    let activity = goal_activity(&projection);
    assert_eq!(activity["available"], true);
    assert_eq!(activity["coverage_partial"], true);
    assert_eq!(activity["linked_window_count"], 16);
    assert_eq!(activity["state"], "unobserved");
}

#[tokio::test]
async fn goal_plan_projection_fails_closed_on_malformed_persisted_goal() {
    let (_temp, db, runtime) = runtime_with_goal_db();
    let bob = auth_context(Some("bob-plan-corrupt"), false);
    let goal_id = create_goal(&runtime, Some(&bob), "bob-plan-corrupt-create");
    {
        let conn = db.conn_for_tests();
        conn.execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();
        conn.execute(
            "UPDATE wc_goals SET lifecycle = 'waiting_validation' WHERE goal_id = ?1",
            [goal_id.as_str()],
        )
        .unwrap();
        conn.execute_batch("PRAGMA ignore_check_constraints = OFF;")
            .unwrap();
    }
    let presented = runtime.present_goal_plan(Some(&bob), goal_id.clone()).await;
    let polled = runtime.present_goal_plan(Some(&bob), goal_id).await;
    assert!(!presented.success);
    assert!(!polled.success);
    assert_eq!(presented.output["error_kind"], "goal_store_unavailable");
    assert_eq!(polled.output["error_kind"], "goal_store_unavailable");
}

#[test]
fn goal_agent_task_link_reauthorizes_task_and_task_completion_never_completes_goal() {
    let (_temp, db, runtime) = runtime_with_goal_db();
    let bob = auth_context(Some("bob-task-goal"), false);
    let alice = auth_context(Some("alice-task-goal"), false);
    let goal_id = create_goal(&runtime, Some(&bob), "bob-task-goal-create");

    let bob_agent = runtime.create_agent_identity(
        Some(&bob),
        "bob-goal-worker".to_string(),
        "Bob Goal Worker".to_string(),
        None,
        Vec::new(),
        "bob-goal-worker-create".to_string(),
    );
    assert!(bob_agent.success);
    let bob_agent_id = bob_agent.output["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string();
    let bob_task = runtime.create_agent_task(
        Some(&bob),
        "Goal-correlated task".to_string(),
        "Complete one explicit durable work chunk.".to_string(),
        Some(bob_agent_id.clone()),
        None,
        None,
        Some("agent:special:reference-is-not-authority".to_string()),
        "bob-goal-task-create".to_string(),
    );
    assert!(bob_task.success, "{:?}", bob_task.output);
    let bob_task_id = bob_task.output["task"]["summary"]["task_id"]
        .as_str()
        .unwrap()
        .to_string();

    let linked = runtime.associate_goal_agent_task(
        Some(&bob),
        goal_id.clone(),
        bob_task_id.clone(),
        "bob-goal-task-link".to_string(),
    );
    assert!(linked.success, "{:?}", linked.output);
    assert_eq!(linked.output["goal"]["summary"]["revision"], 2);
    assert_eq!(linked.output["goal"]["summary"]["agent_task_count"], 1);
    assert_eq!(
        db.conn_for_tests()
            .query_row(
                "SELECT COUNT(*) FROM wc_agent_task_coding_runs",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0,
        "Goal correlation must not dispatch an AgentTask CodingAgentRun"
    );

    let alice_agent = runtime.create_agent_identity(
        Some(&alice),
        "alice-goal-worker".to_string(),
        "Alice Goal Worker".to_string(),
        None,
        Vec::new(),
        "alice-goal-worker-create".to_string(),
    );
    assert!(alice_agent.success);
    let alice_agent_id = alice_agent.output["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string();
    let alice_task = runtime.create_agent_task(
        Some(&alice),
        "Alice private task".to_string(),
        "Private Alice work".to_string(),
        Some(alice_agent_id),
        None,
        None,
        None,
        "alice-private-task-create".to_string(),
    );
    assert!(alice_task.success);
    let alice_task_id = alice_task.output["task"]["summary"]["task_id"]
        .as_str()
        .unwrap()
        .to_string();
    let foreign_link = runtime.associate_goal_agent_task(
        Some(&bob),
        goal_id.clone(),
        alice_task_id,
        "foreign-task-link".to_string(),
    );
    assert!(!foreign_link.success);
    assert_eq!(foreign_link.output["error_kind"], "agent_task_not_found");

    let attempt = runtime.start_agent_task_attempt(
        Some(&bob),
        bob_task_id.clone(),
        bob_agent_id.clone(),
        "bob-goal-task-attempt".to_string(),
    );
    assert!(attempt.success, "{:?}", attempt.output);
    let attempt_id = attempt.output["attempt"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_string();
    let fence = attempt.output["attempt_fence"]
        .as_str()
        .unwrap()
        .to_string();
    let completed = runtime.complete_agent_task_attempt(
        Some(&bob),
        bob_task_id.clone(),
        attempt_id.clone(),
        bob_agent_id.clone(),
        fence,
        1,
        "succeeded".to_string(),
        Some("Task work finished".to_string()),
        None,
        "bob-goal-task-complete".to_string(),
    );
    assert!(completed.success, "{:?}", completed.output);
    assert_eq!(completed.output["task"]["state"], "succeeded");

    let goal = runtime.get_goal(Some(&bob), goal_id.clone());
    assert!(goal.success, "{:?}", goal.output);
    assert_eq!(goal.output["goal"]["summary"]["lifecycle"], "active");
    assert_eq!(goal.output["goal"]["summary"]["revision"], 2);
    assert!(goal.output["goal"]["summary"]["terminal_at_unix_ms"].is_null());

    let (event_id, wake_id): (String, String) = db
        .conn_for_tests()
        .query_row(
            "SELECT e.event_id, w.wake_id
             FROM wc_agent_attention_events e
             JOIN wc_agent_wakes w ON w.source_event_id = e.event_id
             WHERE e.kind = 'agent_task_terminal'
               AND e.goal_id = ?1 AND e.task_id = ?2 AND e.task_attempt_id = ?3
               AND e.target_agent_id = ?4 AND e.terminal_task_state = 'succeeded'
               AND w.trigger_kind = 'attention_event' AND w.state = 'pending'",
            rusqlite::params![goal_id, bob_task_id, attempt_id, bob_agent_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    let endpoint = runtime.attach_agent_endpoint(
        Some(&bob),
        bob_agent_id.clone(),
        "ChatGPT".to_string(),
        Some("goal-attention-runtime".to_string()),
        "goal-attention-runtime-endpoint".to_string(),
    );
    assert!(endpoint.success, "{:?}", endpoint.output);
    let endpoint_id = endpoint.output["endpoint"]["endpoint_id"]
        .as_str()
        .unwrap()
        .to_string();
    let generation = endpoint.output["endpoint"]["controller_generation"]
        .as_i64()
        .unwrap();
    let bootstrap = runtime.bootstrap_agent_conversation(
        Some(&bob),
        bob_agent_id,
        endpoint_id,
        generation,
        None,
        Some(wake_id),
        None,
    );
    assert!(bootstrap.success, "{:?}", bootstrap.output);
    assert_eq!(bootstrap.output["wake"]["trigger_kind"], "attention_event");
    assert_eq!(bootstrap.output["wake"]["event_id"], event_id);
    assert_eq!(bootstrap.output["wake"]["goal_id"], goal_id);
    assert_eq!(bootstrap.output["wake"]["task_id"], bob_task_id);
    assert_eq!(bootstrap.output["wake"]["task_attempt_id"], attempt_id);

    let explicitly_completed = runtime.update_goal(
        Some(&bob),
        goal_id.clone(),
        2,
        None,
        None,
        Some("completed".to_string()),
        Some("Explicit model decision after terminal task attention".to_string()),
        "bob-goal-after-attention-complete".to_string(),
    );
    assert!(
        explicitly_completed.success,
        "{:?}",
        explicitly_completed.output
    );
    assert_eq!(
        explicitly_completed.output["goal"]["summary"]["lifecycle"],
        "completed"
    );
    assert_eq!(
        explicitly_completed.output["goal"]["summary"]["revision"],
        3
    );
}

#[tokio::test]
async fn workflow_session_link_is_identity_only_and_finish_coding_task_does_not_complete_goal() {
    let temp = tempfile::tempdir().unwrap();
    init_git_repo(temp.path());
    commit_file(temp.path(), "README.md", "hello\n", "add readme");
    let db = Arc::new(crate::db::Database::open(&temp.path().join("goal-session.db")).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_communication_database(db);
    let project =
        register_runner_project_at_path(&runtime, "goal-session-finish", "demo", temp.path()).await;
    let auth = auth_context(None, true);
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("Goal-correlated coding work".to_string()),
    );
    let mut limited = auth_context(Some("goal-session-limited"), false);
    limited.scopes = vec![
        SCOPE_COMMUNICATION_READ.to_string(),
        SCOPE_COMMUNICATION_MANAGE.to_string(),
        SCOPE_SESSION_COLLABORATE.to_string(),
    ];
    let limited_goal_id = create_goal(&runtime, Some(&limited), "limited-session-goal-create");
    let unauthorized_link = runtime
        .associate_goal_workflow_session(
            Some(&limited),
            limited_goal_id.clone(),
            session.session_id.clone(),
            "limited-session-goal-link".to_string(),
        )
        .await;
    assert!(
        !unauthorized_link.success,
        "Goal ownership and session:collaborate must not grant bound Project authority"
    );
    let limited_goal = runtime.get_goal(Some(&limited), limited_goal_id);
    assert!(limited_goal.success, "{:?}", limited_goal.output);
    assert_eq!(limited_goal.output["goal"]["summary"]["revision"], 1);
    assert_eq!(
        limited_goal.output["goal"]["summary"]["workflow_session_count"],
        0
    );

    let goal_id = create_goal(&runtime, Some(&auth), "session-goal-create");

    let linked = runtime
        .associate_goal_workflow_session(
            Some(&auth),
            goal_id.clone(),
            session.session_id.clone(),
            "session-goal-link".to_string(),
        )
        .await;
    assert!(linked.success, "{:?}", linked.output);
    assert_eq!(linked.output["goal"]["summary"]["revision"], 2);
    assert_eq!(
        linked.output["goal"]["summary"]["workflow_session_count"],
        1
    );

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::FinishCodingTask {
                        project,
                        session_id,
                        summary_only: true,
                        include_diff: Some(false),
                        include_workspace: Some(true),
                        include_hygiene: Some(false),
                        include_handoff: Some(false),
                        include_validation_summary: Some(false),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, "goal-session-finish").await;
    let show_changes_stdout =
        crate::tool_runtime::framed_clean_show_changes_test_stdout("add readme", false);
    complete_patch_agent_request(
        &runtime,
        "goal-session-finish",
        &request.request_id,
        0,
        &show_changes_stdout,
        "",
    )
    .await;
    let finish = task.await.unwrap();
    assert!(finish.success, "{:?}", finish.error);
    assert_eq!(finish.output["goal_follow_up"]["available"], true);
    assert_eq!(
        finish.output["goal_follow_up"]["goals"][0]["goal_id"],
        goal_id
    );
    assert_eq!(finish.output["goal_follow_up"]["goals"][0]["revision"], 2);
    assert_eq!(
        finish.output["goal_follow_up"]["goals"][0]["next_action"],
        "update_goal"
    );

    let goal = runtime.get_goal(Some(&auth), goal_id);
    assert!(goal.success, "{:?}", goal.output);
    assert_eq!(goal.output["goal"]["summary"]["lifecycle"], "active");
    assert_eq!(goal.output["goal"]["summary"]["revision"], 2);
    assert!(goal.output["goal"]["summary"]["terminal_at_unix_ms"].is_null());
}

use super::support::*;
use crate::auth::scopes::{COMMUNICATION_MANAGE_SCOPES, COMMUNICATION_READ_SCOPES};
use crate::tool_runtime::metadata::{
    ToolApprovalPolicy, ToolAuthorityPolicy, ToolEffect, ToolIdempotency, ToolRisk,
};
use crate::tool_runtime::tool_definition::{lookup_tool_definition, AgentCapability};
use crate::tool_runtime::{ToolCall, ToolRuntime};
use serde_json::json;
use std::sync::Arc;

fn runtime_with_db() -> (tempfile::TempDir, Arc<crate::db::Database>, ToolRuntime) {
    let temp = tempfile::tempdir().unwrap();
    let db = Arc::new(crate::db::Database::open(&temp.path().join("agent-tasks.db")).unwrap());
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
fn agent_task_tools_are_definition_owned_and_do_not_require_runner_or_project_authority() {
    for name in [
        "create_agent_task",
        "list_agent_tasks",
        "read_agent_task",
        "assign_agent_task",
        "start_agent_task_attempt",
        "heartbeat_agent_task_attempt",
        "complete_agent_task_attempt",
    ] {
        let definition = lookup_tool_definition(name).unwrap_or_else(|| panic!("missing {name}"));
        assert!(
            definition.model_spec.is_some(),
            "{name} must own model_spec"
        );
        assert_eq!(definition.category, "agent_task");
        assert!(!definition.metadata.requires_project);
        assert_eq!(definition.agent_capability, None::<AgentCapability>);
        assert_eq!(definition.metadata.provider_id, "control");
    }

    for name in ["list_agent_tasks", "read_agent_task"] {
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

    for name in [
        "create_agent_task",
        "assign_agent_task",
        "start_agent_task_attempt",
        "heartbeat_agent_task_attempt",
        "complete_agent_task_attempt",
    ] {
        let definition = lookup_tool_definition(name).unwrap();
        assert_eq!(definition.metadata.effect, ToolEffect::Mutate);
        assert_eq!(definition.metadata.risk, ToolRisk::WorkflowManage);
        assert_eq!(definition.metadata.approval, ToolApprovalPolicy::Standard);
        assert_eq!(
            definition.metadata.authority,
            ToolAuthorityPolicy::RequireAll(COMMUNICATION_MANAGE_SCOPES)
        );
    }
    assert_eq!(
        lookup_tool_definition("create_agent_task")
            .unwrap()
            .metadata
            .idempotency,
        ToolIdempotency::Keyed
    );
    assert_eq!(
        lookup_tool_definition("assign_agent_task")
            .unwrap()
            .metadata
            .idempotency,
        ToolIdempotency::DesiredState
    );
    assert_eq!(
        lookup_tool_definition("start_agent_task_attempt")
            .unwrap()
            .metadata
            .idempotency,
        ToolIdempotency::Keyed
    );
    assert_eq!(
        lookup_tool_definition("heartbeat_agent_task_attempt")
            .unwrap()
            .metadata
            .idempotency,
        ToolIdempotency::NonIdempotent
    );
    assert_eq!(
        lookup_tool_definition("complete_agent_task_attempt")
            .unwrap()
            .metadata
            .idempotency,
        ToolIdempotency::Keyed
    );
}

#[test]
fn agent_task_output_schemas_publish_bounded_task_and_attempt_contracts() {
    let specs = crate::tool_runtime::registered_tool_specs();
    let spec = |name: &str| {
        specs
            .iter()
            .find(|spec| spec.name == name)
            .unwrap_or_else(|| panic!("missing {name}"))
    };

    let list = spec("list_agent_tasks");
    let task_summary = &list.output_schema["properties"]["output"]["properties"]["tasks"]["items"];
    for field in [
        "task_id",
        "assignee_agent_id",
        "title",
        "source_conversation_id",
        "source_message_id",
        "referenced_project_id",
        "state",
        "latest_attempt",
    ] {
        assert!(
            task_summary["properties"].get(field).is_some(),
            "list_agent_tasks must publish task summary field {field}"
        );
    }
    assert!(task_summary["properties"].get("instruction").is_none());
    assert!(
        task_summary["properties"]["latest_attempt"]["anyOf"][0]["properties"]
            .get("attempt_fence")
            .is_none(),
        "generic Task summaries must never publish attempt_fence"
    );

    let read = spec("read_agent_task");
    let detail = &read.output_schema["properties"]["output"]["properties"]["task"];
    assert!(detail["properties"].get("instruction").is_some());
    assert!(detail["properties"]["summary"]["properties"]
        .get("task_id")
        .is_some());

    let start = spec("start_agent_task_attempt");
    let start_output = &start.output_schema["properties"]["output"]["properties"];
    assert!(start_output.get("attempt_fence").is_some());
    assert!(start_output["attempt"]["properties"]
        .get("attempt_id")
        .is_some());
    assert!(start_output["attempt"]["properties"]
        .get("attempt_fence")
        .is_none());

    for name in [
        "heartbeat_agent_task_attempt",
        "complete_agent_task_attempt",
    ] {
        let output = &spec(name).output_schema["properties"]["output"]["properties"];
        assert!(output["task"]["properties"].get("task_id").is_some());
        assert!(output["attempt"]["properties"].get("attempt_id").is_some());
        assert!(output.get("attempt_fence").is_none());
    }

    for name in ["start_agent_task_attempt", "heartbeat_agent_task_attempt"] {
        let properties = spec(name).input_schema["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("{name} input properties"));
        for forbidden in [
            "lease_expires_at",
            "lease_expires_at_unix_ms",
            "lease_duration_ms",
            "lease_ms",
        ] {
            assert!(
                !properties.contains_key(forbidden),
                "{name} must keep Attempt lease timing Server-authoritative; found {forbidden}"
            );
        }
    }
}

#[test]
fn tool_call_parser_keeps_agent_task_and_connector_task_identities_distinct() {
    let call = ToolCall::from_tool_name(
        "create_agent_task",
        json!({
            "title": "Explicit durable work",
            "instruction": "No Connector Task or Workflow Session identity is inferred.",
            "referenced_project_id": "agent:special:correlation-only",
            "idempotency_key": "parser-task"
        }),
    )
    .unwrap();
    assert_eq!(call.tool_name(), "create_agent_task");

    let task_id = format!("wc_agent_task_{}", "1".repeat(32));
    let attempt_id = format!("wc_agent_task_attempt_{}", "2".repeat(32));
    let fence = format!("wc_agent_task_fence_{}", "3".repeat(32));
    let assignee = format!("wc_dagent_{}", "4".repeat(32));
    let heartbeat = ToolCall::from_tool_name(
        "heartbeat_agent_task_attempt",
        json!({
            "task_id": task_id,
            "attempt_id": attempt_id,
            "assignee_agent_id": assignee,
            "attempt_fence": fence,
            "attempt_controller_generation": 1
        }),
    )
    .unwrap();
    assert_eq!(heartbeat.tool_name(), "heartbeat_agent_task_attempt");

    assert!(ToolCall::from_tool_name(
        "start_agent_task_attempt",
        json!({
            "task_id": "wc_task_connector_identity",
            "assignee_agent_id": format!("wc_dagent_{}", "5".repeat(32)),
            "idempotency_key": "wrong-domain"
        })
    )
    .is_ok(), "ToolCall serde validates shape while canonical id format is enforced by the AgentTask domain at runtime, never remapped to Connector Task");
}

#[test]
fn runtime_surface_exposes_fence_only_for_exact_start_and_never_requires_endpoint() {
    let (_temp, _db, runtime) = runtime_with_db();
    let assignee = create_agent(&runtime, "task-owner");

    let created = runtime.create_agent_task(
        None,
        "Durable task".to_string(),
        "This work survives window and Endpoint replacement.".to_string(),
        Some(assignee.clone()),
        None,
        None,
        Some("agent:special:reference-only".to_string()),
        "runtime-task-create".to_string(),
    );
    assert!(created.success, "{:?}", created.output);
    let task_id = created.output["task"]["summary"]["task_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(created.output["task"]["summary"]["state"], "ready");
    assert_eq!(
        created.output["task"]["summary"]["referenced_project_id"],
        "agent:special:reference-only"
    );

    let listed = runtime.list_agent_tasks(None, Some(assignee.clone()), None, Some(10));
    assert!(listed.success);
    assert_eq!(listed.output["tasks"].as_array().unwrap().len(), 1);
    assert!(listed.output["tasks"][0].get("attempt_fence").is_none());
    assert!(listed.output["tasks"][0]["latest_attempt"].is_null());

    let read = runtime.read_agent_task(None, task_id.clone());
    assert!(read.success);
    assert_eq!(read.output["task"]["summary"]["task_id"], task_id);
    assert!(read.output["task"].get("attempt_fence").is_none());

    let started = runtime.start_agent_task_attempt(
        None,
        task_id.clone(),
        assignee.clone(),
        "runtime-attempt-start".to_string(),
    );
    assert!(started.success, "{:?}", started.output);
    let attempt_id = started.output["attempt"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_string();
    let fence = started.output["attempt_fence"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(fence.starts_with("wc_agent_task_fence_"));
    assert_eq!(
        started.output["attempt"]["attempt_controller_generation"],
        1
    );
    assert!(started.output["attempt"].get("attempt_fence").is_none());

    let heartbeat = runtime.heartbeat_agent_task_attempt(
        None,
        task_id.clone(),
        attempt_id.clone(),
        assignee.clone(),
        fence.clone(),
        1,
    );
    assert!(heartbeat.success, "{:?}", heartbeat.output);
    assert_eq!(heartbeat.output["attempt"]["attempt_id"], attempt_id);

    let completed = runtime.complete_agent_task_attempt(
        None,
        task_id.clone(),
        attempt_id,
        assignee,
        fence,
        1,
        "succeeded".to_string(),
        Some("bounded terminal result".to_string()),
        None,
        "runtime-completion".to_string(),
    );
    assert!(completed.success, "{:?}", completed.output);
    assert_eq!(completed.output["task"]["state"], "succeeded");
    assert_eq!(completed.output["attempt"]["state"], "succeeded");

    let terminal = runtime.read_agent_task(None, task_id);
    assert!(terminal.success);
    assert_eq!(terminal.output["task"]["summary"]["state"], "succeeded");
    assert!(terminal.output["task"].get("attempt_fence").is_none());
}

#[test]
fn agent_task_audit_projection_never_records_instruction_fence_keys_or_terminal_text() {
    const INSTRUCTION: &str = "PRIVATE_AGENT_TASK_INSTRUCTION_DO_NOT_LOG";
    const FENCE: &str = "wc_agent_task_fence_11111111111111111111111111111111";
    const START_KEY: &str = "PRIVATE_START_REPLAY_KEY_DO_NOT_LOG";
    const COMPLETION_KEY: &str = "PRIVATE_COMPLETION_KEY_DO_NOT_LOG";
    const RESULT: &str = "PRIVATE_TERMINAL_RESULT_DO_NOT_LOG";
    const REASON: &str = "PRIVATE_TERMINAL_REASON_DO_NOT_LOG";

    let create_summary = crate::tool_runtime::tool_audit::session_log_arguments_for_tool_request(
        "create_agent_task",
        &json!({
            "title": "private title",
            "instruction": INSTRUCTION,
            "assignee_agent_id": format!("wc_dagent_{}", "a".repeat(32)),
            "referenced_project_id": "agent:special:reference-only",
            "idempotency_key": START_KEY,
        }),
    );
    assert_eq!(create_summary["instruction_bytes"], INSTRUCTION.len());
    assert_eq!(
        create_summary["title_chars"],
        "private title".chars().count()
    );
    assert_eq!(create_summary["idempotency_key_present"], true);

    let complete_summary = crate::tool_runtime::tool_audit::session_log_arguments_for_tool_request(
        "complete_agent_task_attempt",
        &json!({
            "task_id": format!("wc_agent_task_{}", "1".repeat(32)),
            "attempt_id": format!("wc_agent_task_attempt_{}", "2".repeat(32)),
            "assignee_agent_id": format!("wc_dagent_{}", "a".repeat(32)),
            "attempt_fence": FENCE,
            "attempt_controller_generation": 3,
            "outcome": "succeeded",
            "terminal_result": RESULT,
            "terminal_reason": REASON,
            "completion_key": COMPLETION_KEY,
        }),
    );
    assert_eq!(complete_summary["attempt_fence_present"], true);
    assert_eq!(complete_summary["completion_key_present"], true);
    assert_eq!(complete_summary["terminal_result_bytes"], RESULT.len());
    assert_eq!(complete_summary["terminal_reason_bytes"], REASON.len());

    let result_summary = crate::tool_runtime::tool_audit::session_log_result_for_tool(
        "start_agent_task_attempt",
        &json!({
            "task": {
                "task_id": format!("wc_agent_task_{}", "1".repeat(32)),
                "state": "active"
            },
            "attempt": {
                "attempt_id": format!("wc_agent_task_attempt_{}", "2".repeat(32)),
                "attempt_number": 1,
                "state": "active",
                "attempt_controller_generation": 1,
                "terminal_result": RESULT,
                "terminal_reason": REASON
            },
            "attempt_fence": FENCE,
            "replayed": false,
            "state_changed": true
        }),
    );
    let serialized = serde_json::to_string(&json!({
        "create": create_summary,
        "complete": complete_summary,
        "result": result_summary,
    }))
    .unwrap();
    for private in [
        INSTRUCTION,
        FENCE,
        START_KEY,
        COMPLETION_KEY,
        RESULT,
        REASON,
        "private title",
    ] {
        assert!(!serialized.contains(private), "audit leaked {private}");
    }
}

#[test]
fn foreign_runtime_task_ids_are_existence_hidden_and_project_reference_grants_nothing() {
    let (_temp, _db, runtime) = runtime_with_db();
    let bob = auth_context(Some("bob"), false);
    let alice = auth_context(Some("alice"), false);
    let bob_agent = runtime.create_agent_identity(
        Some(&bob),
        "bob-task-agent".to_string(),
        "Bob Task Agent".to_string(),
        None,
        Vec::new(),
        "bob-task-agent-create".to_string(),
    );
    assert!(bob_agent.success);
    let bob_agent_id = bob_agent.output["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string();
    let created = runtime.create_agent_task(
        Some(&bob),
        "Private durable task".to_string(),
        "Private task instruction".to_string(),
        Some(bob_agent_id),
        None,
        None,
        Some("agent:special:project-not-authority".to_string()),
        "bob-private-task".to_string(),
    );
    assert!(created.success);
    let task_id = created.output["task"]["summary"]["task_id"]
        .as_str()
        .unwrap()
        .to_string();
    let missing_id = format!("wc_agent_task_{}", "f".repeat(32));

    let foreign = runtime.read_agent_task(Some(&alice), task_id);
    let missing = runtime.read_agent_task(Some(&alice), missing_id);
    assert!(!foreign.success);
    assert!(!missing.success);
    assert_eq!(foreign.output["error_kind"], "agent_task_not_found");
    assert_eq!(foreign.output["error_kind"], missing.output["error_kind"]);
    assert_eq!(foreign.error, missing.error);

    let definition = lookup_tool_definition("create_agent_task").unwrap();
    assert!(!definition.metadata.requires_project);
    assert_eq!(definition.agent_capability, None);
    assert!(!matches!(
        definition.metadata.risk,
        ToolRisk::ProjectWrite | ToolRisk::JobRun
    ));
}

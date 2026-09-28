use salvo::http::StatusCode;
use salvo::test::{ResponseExt, TestClient};
use salvo::Service;
use serde_json::{json, Value};
use std::sync::Arc;

fn single_model_ergonomics(
    db: &crate::Database,
    action_session_id: &str,
    expected_tool: &str,
) -> Value {
    let events = db.list_action_events(action_session_id, 20).unwrap();
    assert_eq!(
        events.len(),
        1,
        "one outer tool call must create one ActionAudit row"
    );
    assert_eq!(events[0].operation.as_deref(), Some(expected_tool));
    let summary: Value = serde_json::from_str(&events[0].summary_json).unwrap();
    summary
        .get("model_ergonomics")
        .cloned()
        .unwrap_or_else(|| panic!("missing generic telemetry in summary: {summary}"))
}

#[tokio::test]
async fn api_model_ergonomics_success_is_exact_and_queryable() {
    let config = super::test_config(Some("secret"));
    let (_db_tmp, db) = super::test_db();
    let project_tmp = tempfile::tempdir().unwrap();
    let runtime = Arc::new(super::runtime_with_local_project(
        project_tmp.path(),
        "demo",
    ));
    let service = Service::new(super::build_projects_router(config, db.clone(), runtime));

    let mut response = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth("secret")
        .add_header("x-action-session-id", "ergonomics-success", true)
        .json(&json!({"tool": "tool_manifest", "params": {"intent": "audit"}}))
        .send(&service)
        .await;
    assert_eq!(super::effective_status(&response), StatusCode::OK);
    let body: Value = response.take_json().await.unwrap();
    assert_eq!(body["success"], true);

    let telemetry = single_model_ergonomics(&db, "ergonomics-success", "tool_manifest");
    assert_eq!(telemetry["schema_version"], 12);
    assert_eq!(telemetry["tool_name"], "tool_manifest");
    assert_eq!(telemetry["tool_category"], "runtime");
    assert_eq!(telemetry["success"], true);
    assert!(telemetry["duration_ms"].as_u64().is_some());
    assert_eq!(
        telemetry["serialized_result_bytes"].as_u64().unwrap(),
        serde_json::to_vec(&body).unwrap().len() as u64,
        "API telemetry must count the exact final ToolResult UTF-8 serialization"
    );
    assert!(telemetry["error_kind"].is_null());
    assert!(telemetry["failure_kind"].is_null());
    assert!(telemetry["recovery_kind"].is_null());
    assert!(telemetry.get("result_truncated").is_none());
}

#[tokio::test]
async fn api_model_ergonomics_failure_uses_structured_kinds_without_private_text() {
    let config = super::test_config(Some("secret"));
    let (_db_tmp, db) = super::test_db();
    let project_tmp = tempfile::tempdir().unwrap();
    let runtime = Arc::new(super::runtime_with_local_project(
        project_tmp.path(),
        "demo",
    ));
    let service = Service::new(super::build_projects_router(config, db.clone(), runtime));
    let private_project = "PRIVATE-command-path-query-token";

    let mut response = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth("secret")
        .add_header("x-action-session-id", "ergonomics-failure", true)
        .json(&json!({"tool": "project_overview", "params": {"project": private_project}}))
        .send(&service)
        .await;
    assert_eq!(super::effective_status(&response), StatusCode::BAD_REQUEST);
    let body: Value = response.take_json().await.unwrap();
    assert_eq!(body["success"], false);
    assert_eq!(body["output"]["error_kind"], "unknown_project");
    assert_eq!(body["output"]["recovery_kind"], "fix_input");

    let telemetry = single_model_ergonomics(&db, "ergonomics-failure", "project_overview");
    assert_eq!(telemetry["success"], false);
    assert_eq!(telemetry["error_kind"], "unknown_project");
    assert!(telemetry["failure_kind"].is_null());
    assert_eq!(telemetry["recovery_kind"], "fix_input");
    assert_eq!(
        telemetry["serialized_result_bytes"].as_u64().unwrap(),
        serde_json::to_vec(&body).unwrap().len() as u64
    );
    let serialized = serde_json::to_string(&telemetry).unwrap();
    for forbidden in [private_project, "command", "path", "query", "token"] {
        assert!(
            !serialized.contains(forbidden),
            "generic telemetry leaked arbitrary/private text {forbidden}: {serialized}"
        );
    }
}

#[tokio::test]
async fn api_pre_result_invalid_arguments_still_counts_without_fabricated_bytes() {
    let config = super::test_config(Some("secret"));
    let (_db_tmp, db) = super::test_db();
    let project_tmp = tempfile::tempdir().unwrap();
    let runtime = Arc::new(super::runtime_with_local_project(
        project_tmp.path(),
        "demo",
    ));
    let service = Service::new(super::build_projects_router(config, db.clone(), runtime));

    let mut response = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth("secret")
        .add_header("x-action-session-id", "ergonomics-invalid", true)
        .json(&json!({"tool": "read_files"}))
        .send(&service)
        .await;
    assert_eq!(super::effective_status(&response), StatusCode::BAD_REQUEST);
    let body: Value = response.take_json().await.unwrap();
    assert!(body["error"].is_string());

    let telemetry = single_model_ergonomics(&db, "ergonomics-invalid", "read_files");
    assert_eq!(telemetry["success"], false);
    assert_eq!(telemetry["error_kind"], "invalid_arguments");
    assert!(telemetry["serialized_result_bytes"].is_null());
    assert!(telemetry["failure_kind"].is_null());
    assert!(telemetry["recovery_kind"].is_null());
    assert!(telemetry["execution_state"].is_null());
}

#[tokio::test]
async fn api_batch_call_records_one_generic_outer_invocation() {
    let config = super::test_config(Some("secret"));
    let (_db_tmp, db) = super::test_db();
    let project_tmp = tempfile::tempdir().unwrap();
    let (runtime, registry) = super::register_import_agent_with_capabilities(
        project_tmp.path(),
        Some(crate::runner_protocol::RunnerCapabilities {
            file_read: true,
            ..Default::default()
        }),
    )
    .await;
    let executor = super::spawn_startup_agent_executor(registry);
    let service = Service::new(super::build_projects_router(config, db.clone(), runtime));

    let mut response = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth("secret")
        .add_header("x-action-session-id", "ergonomics-batch", true)
        .json(&json!({
            "tool": "read_files",
            "params": {
                "project": "agent:importer:demo",
                "items": [
                    {"path": "missing-a.rs"},
                    {"path": "missing-b.rs"}
                ]
            }
        }))
        .send(&service)
        .await;
    let status = super::effective_status(&response);
    let body: Value = response.take_json().await.unwrap();
    executor.abort();

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["success"], true);
    assert_eq!(body["output"]["items"].as_array().unwrap().len(), 2);
    assert!(body["output"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| item["success"] == false));
    let telemetry = single_model_ergonomics(&db, "ergonomics-batch", "read_files");
    assert_eq!(telemetry["tool_name"], "read_files");
    assert_eq!(telemetry["success"], true);
}

#[tokio::test]
async fn api_work_on_project_preferences_persist_as_privacy_bounded_action_audit_facts() {
    let config = super::test_config(Some("secret"));
    let (_db_tmp, db) = super::test_db();
    let project_tmp = tempfile::tempdir().unwrap();
    let (runtime, registry) = super::register_import_agent(project_tmp.path()).await;
    let executor = super::spawn_startup_agent_executor(registry);
    let service = Service::new(super::build_projects_router(config, db.clone(), runtime));
    let private_instruction = "PRIVATE_API_WORK_ON_PROJECT_INSTRUCTION";
    let project = "agent:importer:demo";

    let mut response = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth("secret")
        .add_header("x-action-session-id", "ergonomics-work-on-project", true)
        .json(&json!({
            "tool": "work_on_project",
            "params": {
                "project": project,
                "instruction": private_instruction,
                "guidance_profile": "host_code_mode",
                "include_extension_catalog": false
            }
        }))
        .send(&service)
        .await;
    let status = super::effective_status(&response);
    let body: Value = response.take_json().await.unwrap();
    executor.abort();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["success"], true, "{body}");

    let telemetry = single_model_ergonomics(&db, "ergonomics-work-on-project", "work_on_project");
    assert_eq!(telemetry["schema_version"], 12);
    let facts = &telemetry["work_on_project"];
    assert_eq!(facts["resume_requested"], false);
    assert_eq!(facts["source"], "project");
    assert_eq!(facts["mode"], "checkout");
    assert_eq!(facts["mode_explicit"], false);
    assert_eq!(facts["base_ref_present"], false);
    assert_eq!(facts["guidance_profile"], "host_code_mode");
    assert_eq!(facts["guidance_profile_explicit"], true);
    assert_eq!(facts["include_extension_catalog"], false);
    assert_eq!(facts["include_extension_catalog_explicit"], true);
    let invocation = &telemetry["invocation"];
    assert_eq!(invocation["context_present"], false);
    assert_eq!(invocation["control_present"], false);
    let bootstrap = &telemetry["bootstrap"];
    assert!(bootstrap.is_object());
    assert!(bootstrap["instructions_available"].is_null());
    assert!(bootstrap["workflow_available"].is_null());
    assert!(bootstrap["instruction_observation_status"].is_string());
    assert!(bootstrap["workspace_status"].is_string());
    assert!(bootstrap["semantic_supported"].is_boolean());
    let serialized = serde_json::to_string(&telemetry).unwrap();
    for forbidden in [private_instruction, project] {
        assert!(
            !serialized.contains(forbidden),
            "persisted API model ergonomics leaked {forbidden}: {serialized}"
        );
    }
}

#[tokio::test]
async fn action_audit_sink_failure_never_changes_success_or_failure_tool_result() {
    let config = super::test_config(Some("secret"));
    let (_db_tmp, db) = super::test_db();
    let project_tmp = tempfile::tempdir().unwrap();
    let runtime = Arc::new(super::runtime_with_local_project(
        project_tmp.path(),
        "demo",
    ));
    let service = Service::new(super::build_projects_router(config, db.clone(), runtime));
    db.conn_for_tests()
        .execute("DROP TABLE action_events", [])
        .unwrap();

    let mut success = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth("secret")
        .json(&json!({"tool": "tool_manifest", "params": {"intent": "audit"}}))
        .send(&service)
        .await;
    assert_eq!(super::effective_status(&success), StatusCode::OK);
    let success_body: Value = success.take_json().await.unwrap();
    assert_eq!(success_body["success"], true);
    assert!(success_body["output"]["tools"].is_array());

    let mut failure = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth("secret")
        .json(&json!({"tool": "project_overview", "params": {"project": "missing-project"}}))
        .send(&service)
        .await;
    assert_eq!(super::effective_status(&failure), StatusCode::BAD_REQUEST);
    let failure_body: Value = failure.take_json().await.unwrap();
    assert_eq!(failure_body["success"], false);
    assert_eq!(failure_body["output"]["error_kind"], "unknown_project");
    assert_eq!(failure_body["output"]["recovery_kind"], "fix_input");
}

#[tokio::test]
async fn api_edit_compaction_preserves_canonical_action_audit_and_telemetry() {
    use crate::runner_protocol::{RunnerCapabilities, RunnerPollRequest, RunnerResultRequest};
    let config = super::test_config(Some("secret"));
    let (_db_tmp, db) = super::test_db();
    let project_tmp = tempfile::tempdir().unwrap();
    let (runtime, registry) = super::register_import_agent_with_capabilities(
        project_tmp.path(),
        Some(RunnerCapabilities {
            file_write: true,
            apply_text_edit_local_guard_without_sha: true,
            ..Default::default()
        }),
    )
    .await;
    let service = Service::new(super::build_projects_router(config, db.clone(), runtime));
    let runner = async {
        let request = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if let Some(request) = registry
                    .poll(RunnerPollRequest {
                        client_id: "importer".into(),
                        runner_instance_id: "inst-import".into(),
                    })
                    .await
                    .unwrap()
                {
                    break request;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("edit request before deadline");
        assert_eq!(request.kind, "file_apply_text_edits");
        registry.complete(RunnerResultRequest {
            client_id: "importer".into(), runner_instance_id: "inst-import".into(),
            request_id: request.request_id, exit_code: Some(0),
            stdout: Some(json!({"dry_run":false,"applied_count":1,"planned_count":1,
                "changed":true,"would_change":true,"changed_paths":["new.rs"],
                "files":[{"index":0,"kind":"create","path":"new.rs","to_path":null,
                    "old_sha256":null,"new_sha256":"a".repeat(64),"changed":true,"would_change":true,"edits":[]}]
            }).to_string()), stderr: Some(String::new()), stdout_truncated:false,
            stderr_truncated:false, duration_ms:Some(1), error:None,
        }).await.unwrap();
    };
    let request = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth("secret")
        .add_header("x-action-session-id", "edit-compaction", true)
        .json(
            &json!({"tool":"edit_project_files", "params":{"project":"agent:importer:demo",
            "changes":[{"kind":"create","path":"new.rs","content":"hello"}]}}),
        );
    let (mut response, ()) = tokio::join!(request.send(&service), runner);
    let body: Value = response.take_json().await.unwrap();
    assert_eq!(body["success"], true, "{body}");
    assert!(body["output"].get("dry_run").is_none());
    assert!(body["output"].get("execution_state").is_none());
    let events = db.list_action_events("edit-compaction", 20).unwrap();
    assert_eq!(events.len(), 1);
    let summary: Value = serde_json::from_str(&events[0].summary_json).unwrap();
    assert_eq!(summary["output"]["dry_run"], false);
    assert_eq!(summary["output"]["execution_state"], "completed");
    assert_eq!(summary["output"]["applied_count"], 1);
    assert_eq!(summary["output"]["state_changed"], true);
    assert_eq!(summary["output"]["files"][0]["new_sha256"], "a".repeat(64));
    assert_eq!(summary["model_ergonomics"]["edit_outcome"], "applied");
    assert_eq!(summary["model_ergonomics"]["execution_state"], "completed");
    assert_eq!(
        summary["model_ergonomics"]["serialized_result_bytes"],
        serde_json::to_vec(&body).unwrap().len()
    );
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &body,
        &crate::tool_runtime::registry::output_schema_for_tool("edit_project_files"),
    )
    .unwrap();
}

#[tokio::test]
async fn api_execution_compaction_preserves_canonical_action_audit_and_telemetry() {
    use crate::runner_protocol::{
        RunnerCapabilities, RunnerPollRequest, RunnerResultPayload, RunnerResultRequest,
        ShellCommandExecutionState,
    };
    for tool in ["run_process", "run_shell"] {
        let config = super::test_config(Some("secret"));
        let (_db_tmp, db) = super::test_db();
        let project_tmp = tempfile::tempdir().unwrap();
        let (runtime, registry) = super::register_import_agent_with_capabilities(
            project_tmp.path(),
            Some(RunnerCapabilities {
                structured_process_argv: true,
                shell: true,
                ..Default::default()
            }),
        )
        .await;
        let service = Service::new(super::build_projects_router(config, db.clone(), runtime));
        let runner = async {
            let request = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    if let Some(request) = registry
                        .poll(RunnerPollRequest {
                            client_id: "importer".into(),
                            runner_instance_id: "inst-import".into(),
                        })
                        .await
                        .unwrap()
                    {
                        break request;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("execution request before deadline");
            assert_eq!(request.process.is_some(), tool == "run_process");
            registry
                .complete(RunnerResultPayload {
                    result: RunnerResultRequest {
                        client_id: "importer".into(),
                        runner_instance_id: "inst-import".into(),
                        request_id: request.request_id,
                        exit_code: Some(0),
                        stdout: Some("PRIVATE_STDOUT".into()),
                        stderr: Some("PRIVATE_STDERR".into()),
                        stdout_truncated: false,
                        stderr_truncated: false,
                        duration_ms: Some(1),
                        error: None,
                    },
                    command_execution_state: Some(ShellCommandExecutionState::Completed),
                    mcp_gateway: None,
                    plugin_gateway: None,
                    coding_agent: None,
                })
                .await
                .unwrap();
        };
        let request = TestClient::post("http://localhost/api/tools/call")
        .bearer_auth("secret")
        .add_header("x-action-session-id", "execution-compaction", true)
        .json(
            &json!({"tool":tool, "params": if tool == "run_process" {
                json!({"project":"agent:importer:demo", "executable":"private-command", "args":[], "timeout_secs":30, "sync_wait_secs":30})
            } else {
                json!({"project":"agent:importer:demo", "command":"echo PRIVATE_COMMAND", "timeout_secs":30, "sync_wait_secs":30})
            }}),
        );
        let (mut response, ()) = tokio::join!(request.send(&service), runner);
        let body: Value = response.take_json().await.unwrap();
        assert_eq!(body["success"], true, "{body}");
        assert!(body["output"].get("dry_run").is_none());
        assert!(body["output"].get("execution_state").is_none());
        let events = db.list_action_events("execution-compaction", 20).unwrap();
        assert_eq!(events.len(), 1);
        let summary: Value = serde_json::from_str(&events[0].summary_json).unwrap();
        assert_eq!(summary["output"]["execution_state"], "completed");
        assert_eq!(summary["output"]["command_started"], true);
        assert_eq!(summary["output"]["command_completed"], true);
        assert_eq!(summary["output"]["command_ok"], true);
        assert!(!summary.to_string().contains("PRIVATE"));
        assert_eq!(body["output"]["stdout_tail"], "PRIVATE_STDOUT");
        assert_eq!(body["output"]["stderr_tail"], "PRIVATE_STDERR");
        assert_eq!(summary["model_ergonomics"]["execution_state"], "completed");
        assert_eq!(
            summary["model_ergonomics"]["serialized_result_bytes"],
            serde_json::to_vec(&body).unwrap().len()
        );
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &body,
            &crate::tool_runtime::registry::output_schema_for_tool(tool),
        )
        .unwrap();
    }
}

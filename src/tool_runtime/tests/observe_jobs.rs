//! Phase D bounded batch Job observation.

use super::super::*;
use super::support::*;
use crate::shell_protocol::ShellClientCapabilities;
use serde_json::json;
use std::time::{Duration, Instant};

fn item(job_id: &str, token: Option<String>) -> ObserveJobsItem {
    ObserveJobsItem {
        job_id: job_id.to_string(),
        after_observation_token: token,
    }
}

async fn register_and_start_agent_job(
    runtime: &ToolRuntime,
    client_id: &str,
) -> (
    String,
    crate::shell_protocol::ShellAgentShellRequest,
    crate::auth::AuthContext,
) {
    let caps = ShellClientCapabilities {
        async_jobs: true,
        async_shell_jobs: true,
        ..Default::default()
    };
    register_agent(runtime, client_id, None, caps).await;
    let auth = bootstrap_auth_context();
    let started = runtime
        .dispatch_with_auth(
            ToolCall::RunJob {
                project: agent_test_project_id(client_id),
                command: format!("echo {client_id}"),
                session_id: None,
                timeout_secs: Some(60),
                cwd: None,
                purpose: Some(ExecutionPurpose::Diagnostic),
                shell: None,
            },
            Some(&auth),
        )
        .await;
    assert!(started.success, "{:?}", started.error);
    let job_id = started.output["job_id"].as_str().unwrap().to_string();
    let request = wait_for_patch_agent_request(runtime, client_id).await;
    assert_eq!(request.job_id.as_deref(), Some(job_id.as_str()));
    (job_id, request, auth)
}

async fn start_owned_agent_job(
    runtime: &ToolRuntime,
    client_id: &str,
    project_id: &str,
    auth: &crate::auth::AuthContext,
) -> String {
    let caps = ShellClientCapabilities {
        async_jobs: true,
        async_shell_jobs: true,
        ..Default::default()
    };
    register_agent_projects_for_auth(
        runtime,
        client_id,
        auth,
        caps,
        vec![registered_project(
            project_id,
            &format!("/tmp/{project_id}"),
        )],
    )
    .await;
    let started = runtime
        .dispatch_with_auth(
            ToolCall::RunJob {
                project: format!("agent:{client_id}:{project_id}"),
                command: format!("echo {client_id}"),
                session_id: None,
                timeout_secs: Some(60),
                cwd: None,
                purpose: None,
                shell: None,
            },
            Some(auth),
        )
        .await;
    assert!(started.success, "{:?}", started.error);
    let job_id = started.output["job_id"].as_str().unwrap().to_string();
    let request = wait_for_agent_request_for_client(runtime, client_id).await;
    assert_eq!(request.job_id.as_deref(), Some(job_id.as_str()));
    job_id
}

#[test]
fn observe_jobs_tool_call_enforces_batch_and_scalar_bounds() {
    for count in [1, 8] {
        let items = (0..count)
            .map(|index| json!({"job_id": format!("job-{index}")}))
            .collect::<Vec<_>>();
        let parsed = ToolCall::from_tool_name("observe_jobs", json!({"items": items})).unwrap();
        assert!(matches!(
            parsed,
            ToolCall::ObserveJobs {
                tail_lines: 40,
                wait_secs: None,
                ..
            }
        ));
    }
    for count in [0, 9] {
        let items = (0..count)
            .map(|index| json!({"job_id": format!("job-{index}")}))
            .collect::<Vec<_>>();
        assert!(ToolCall::from_tool_name("observe_jobs", json!({"items": items})).is_err());
    }

    assert!(ToolCall::from_tool_name(
        "observe_jobs",
        json!({"items": [{"job_id": "job", "unknown": true}]})
    )
    .is_err());
    assert!(ToolCall::from_tool_name(
        "observe_jobs",
        json!({"items": [{"job_id": "job"}], "unknown": true})
    )
    .is_err());
    assert!(ToolCall::from_tool_name(
        "observe_jobs",
        json!({"items": [{"job_id": "same"}, {"job_id": "same"}]})
    )
    .is_err());

    let max_token = "x".repeat(192);
    assert!(ToolCall::from_tool_name(
        "observe_jobs",
        json!({"items": [{"job_id": "job", "after_observation_token": max_token}]})
    )
    .is_ok());
    assert!(ToolCall::from_tool_name(
        "observe_jobs",
        json!({"items": [{"job_id": "job", "after_observation_token": "x".repeat(193)}]})
    )
    .is_err());

    for tail_lines in [1, 200] {
        assert!(ToolCall::from_tool_name(
            "observe_jobs",
            json!({"items": [{"job_id": "job"}], "tail_lines": tail_lines})
        )
        .is_ok());
    }
    for tail_lines in [0, 201] {
        assert!(ToolCall::from_tool_name(
            "observe_jobs",
            json!({"items": [{"job_id": "job"}], "tail_lines": tail_lines})
        )
        .is_err());
    }
    for wait_secs in [1, 60] {
        assert!(ToolCall::from_tool_name(
            "observe_jobs",
            json!({"items": [{"job_id": "job"}], "wait_secs": wait_secs})
        )
        .is_ok());
    }
    for wait_secs in [0, 61] {
        assert!(ToolCall::from_tool_name(
            "observe_jobs",
            json!({"items": [{"job_id": "job"}], "wait_secs": wait_secs})
        )
        .is_err());
    }
}

#[tokio::test]
async fn observe_jobs_direct_dispatch_rejects_duplicates_before_observation() {
    let runtime = test_runtime();
    let started = Instant::now();
    let result = runtime
        .dispatch(ToolCall::ObserveJobs {
            items: vec![item("duplicate", None), item("duplicate", None)],
            tail_lines: 40,
            wait_secs: Some(60),
        })
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("duplicate"));
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn observe_jobs_schema_catalog_permission_and_audit_are_public_and_token_safe() {
    let specs = registered_tool_specs();
    let spec = specs
        .iter()
        .find(|spec| spec.name == "observe_jobs")
        .expect("model-visible observe_jobs ToolSpec");
    assert_eq!(spec.input_schema["properties"]["items"]["minItems"], 1);
    assert_eq!(spec.input_schema["properties"]["items"]["maxItems"], 8);
    assert_eq!(spec.input_schema["properties"]["tail_lines"]["default"], 40);
    assert_eq!(
        spec.input_schema["properties"]["tail_lines"]["maximum"],
        200
    );
    assert_eq!(
        spec.input_schema["properties"]["items"]["items"]["additionalProperties"],
        false
    );
    assert_eq!(
        spec.output_schema["properties"]["output"]["anyOf"][0]["properties"]["wake_reason"]["enum"],
        json!(["immediate", "updated", "terminal", "item_error", "timeout"])
    );
    let observation = &spec.output_schema["properties"]["output"]["anyOf"][0]["properties"]
        ["items"]["items"]["properties"]["output"]["anyOf"][0];
    assert_eq!(
        observation["properties"]["log_delta_status"]["enum"],
        json!(["baseline", "delta", "unchanged", "reset"])
    );
    assert_eq!(
        observation["properties"]["observation_token"]["maxLength"],
        crate::job_observation::MAX_JOB_OBSERVATION_TOKEN_LEN
    );
    for required in [
        "log_delta_status",
        "stdout_delta_reset",
        "stderr_delta_reset",
    ] {
        assert!(
            observation["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == required),
            "observe_jobs item output must require {required}"
        );
    }

    let definition = super::super::tool_definition::lookup_tool_definition("observe_jobs").unwrap();
    assert!(definition.visibility.is_model_visible());
    assert_eq!(definition.category, "job");
    assert_eq!(
        definition.metadata.authority,
        crate::tool_runtime::metadata::ToolAuthorityPolicy::Require(
            crate::auth::SCOPE_RUNTIME_READ
        )
    );
    assert_eq!(
        definition.metadata.effect,
        crate::tool_runtime::metadata::ToolEffect::Observe
    );
    let manifest = super::super::surface::registered_tool_categories();
    assert!(manifest["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|name| name == "observe_jobs"));

    let opaque = "wjob1:a:job:private_epoch_body:7";
    let call = ToolCall::ObserveJobs {
        items: vec![item("job", Some(opaque.to_string()))],
        tail_lines: 40,
        wait_secs: Some(5),
    };
    let summary = call.session_log_arguments();
    assert_eq!(summary["item_count"], 1);
    assert_eq!(summary["token_count"], 1);
    assert_eq!(summary["job_ids"], json!(["job"]));
    assert_eq!(summary["tail_lines"], 40);
    assert_eq!(summary["wait_secs"], 5);
    assert!(!serde_json::to_string(&summary).unwrap().contains(opaque));

    let raw_summary = super::super::tool_audit::session_log_arguments_for_tool_request(
        "observe_jobs",
        &json!({
            "items": [{"job_id": "job", "after_observation_token": opaque}],
            "tail_lines": 40,
            "wait_secs": 5
        }),
    );
    assert_eq!(raw_summary["token_count"], 1);
    assert!(!serde_json::to_string(&raw_summary)
        .unwrap()
        .contains(opaque));
    let defensive = super::super::sessions::session_input_summary_for_tool(
        "observe_jobs",
        &json!({
            "items": [{"job_id": "job", "after_observation_token": opaque}],
            "tail_lines": 40,
            "wait_secs": 5
        }),
    );
    assert!(!serde_json::to_string(&defensive).unwrap().contains(opaque));
}

#[tokio::test]
async fn observe_jobs_inaccessible_and_unknown_items_are_indistinguishable() {
    let runtime = test_runtime();
    let auth_a = shared_key_auth_context("observe-owner-a");
    let auth_b = shared_key_auth_context("observe-owner-b");
    let job_a = start_owned_agent_job(&runtime, "observe-owner-a", "project-a", &auth_a).await;
    let job_b = start_owned_agent_job(&runtime, "observe-owner-b", "project-b", &auth_b).await;

    let result = runtime
        .dispatch_with_auth(
            ToolCall::ObserveJobs {
                items: vec![
                    item(&job_b, None),
                    item(&job_a, None),
                    item("missing-owned-job", None),
                ],
                tail_lines: 40,
                wait_secs: Some(5),
            },
            Some(&auth_b),
        )
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["items"][0]["success"], true);
    for index in [1, 2] {
        assert_eq!(result.output["items"][index]["success"], false);
        assert_eq!(result.output["items"][index]["error_kind"], "unknown_job");
        let error = result.output["items"][index]["error"].as_str().unwrap();
        assert!(error.starts_with("unknown job:"));
        assert!(!error.contains("authorization"));
        assert!(!error.contains("owner"));
        assert!(!error.contains("project-"));
    }
}

#[tokio::test]
async fn observe_jobs_mixed_success_result_matches_declared_output_schema_and_enqueues_nothing() {
    let runtime = test_runtime();
    let (agent_job, _request, auth) =
        register_and_start_agent_job(&runtime, "observe-no-enqueue").await;
    let initial = runtime
        .job_log_for_auth(agent_job.clone(), None, Some(40), Some(&auth), None, None)
        .await;
    let token = initial.output["observation_token"]
        .as_str()
        .unwrap()
        .to_string();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ObserveJobs {
                items: vec![
                    item(&agent_job, Some(token)),
                    item("unknown-observe-job", Some("malformed".to_string())),
                ],
                tail_lines: 40,
                wait_secs: Some(5),
            },
            Some(&auth),
        )
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["succeeded_count"], 1);
    assert_eq!(result.output["failed_count"], 1);
    assert!(probe_patch_agent_request(&runtime, "observe-no-enqueue")
        .await
        .is_none());

    let schema = super::super::registry::output_schema_for_tool("observe_jobs");
    let value = serde_json::to_value(&result).unwrap();
    assert!(
        super::super::startup_brief::validate_schema_instance_for_test(&value, &schema).is_ok(),
        "mixed observe_jobs result did not satisfy output schema: {value}"
    );
}

#[test]
fn observe_jobs_session_sanitizer_removes_nested_token_bodies() {
    let opaque = "wjob1:a:job:opaque-private-body:123";
    let summary = super::super::sessions::session_input_summary_for_tool(
        "observe_jobs",
        &json!({
            "items": [
                {"job_id": "job", "after_observation_token": opaque}
            ],
            "tail_lines": 40,
            "wait_secs": 5
        }),
    );
    let serialized = serde_json::to_string(&summary).unwrap();
    assert!(!serialized.contains(opaque));
    assert_eq!(summary["items"][0]["job_id"], "job");
    assert!(summary["items"][0].get("after_observation_token").is_none());
}

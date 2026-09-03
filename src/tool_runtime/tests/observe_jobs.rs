//! Phase D bounded batch Job observation.

use super::super::*;
use super::support::*;
use crate::runner_protocol::{
    RunnerCapabilities, RunnerJobUpdateRequest, RunnerRequest, ShellJobActivity,
    ShellJobActivityPhase,
    ShellJobActivitySource, ShellJobActivityState,
};
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
    crate::runner_protocol::RunnerRequest,
    crate::auth::AuthContext,
) {
    let caps = RunnerCapabilities {
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

fn process_activity() -> ShellJobActivity {
    ShellJobActivity {
        state: ShellJobActivityState::Working,
        phase: ShellJobActivityPhase::ProcessRunning,
        source: ShellJobActivitySource::RunnerExecution,
    }
}

async fn update_observed_job(
    runtime: &ToolRuntime,
    client_id: &str,
    request: &RunnerRequest,
    status: &str,
    stdout_chunk: Option<&str>,
    activity: Option<ShellJobActivity>,
    finished: bool,
) {
    runtime
        .runner_registry
        .update_job(RunnerJobUpdateRequest {
            client_id: client_id.to_string(),
            runner_instance_id: "inst".to_string(),
            update_seq: None,
            job_id: request.job_id.clone().expect("Job request id"),
            request_id: Some(request.request_id.clone()),
            status: status.to_string(),
            stdout_chunk: stdout_chunk.map(str::to_string),
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code: finished.then_some(0),
            duration_ms: finished.then_some(25),
            error: None,
            command_execution_state: None,
            validation_progress: None,
            activity,
            finished,
        })
        .await
        .unwrap();
}

async fn observation_token(
    runtime: &ToolRuntime,
    job_id: &str,
    auth: &crate::auth::AuthContext,
) -> String {
    runtime
        .job_log_for_auth(job_id.to_string(), None, Some(40), Some(auth), None, None)
        .await
        .output["observation_token"]
        .as_str()
        .unwrap()
        .to_string()
}

fn assert_item_has_no_wait_metadata(item: &serde_json::Value) {
    let output = item["output"].as_object().expect("successful Job snapshot");
    assert!(!output.contains_key("wait_outcome"));
    assert!(!output.contains_key("waited_ms"));
}

async fn start_owned_agent_job(
    runtime: &ToolRuntime,
    client_id: &str,
    project_id: &str,
    auth: &crate::auth::AuthContext,
) -> String {
    let caps = RunnerCapabilities {
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
    let request = wait_for_runner_request_for_client(runtime, client_id).await;
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
    let output = &spec.output_schema["properties"]["output"]["anyOf"][0];
    assert_eq!(
        output["properties"]["wait"]["properties"]["outcome"]["enum"],
        json!(["immediate", "updated", "terminal", "item_error", "timeout"])
    );
    assert!(output["properties"].get("wake_reason").is_none());
    assert!(output["properties"].get("waited_ms").is_none());
    let observation = &output["properties"]["items"]["items"]["properties"]["output"]["anyOf"][0];
    assert!(observation["properties"].get("wait_outcome").is_none());
    assert!(observation["properties"].get("waited_ms").is_none());
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
    assert_eq!(result.output["wait"]["outcome"], "item_error");
    assert_eq!(result.output["wait"]["waited_ms"], 0);
    assert_item_has_no_wait_metadata(&result.output["items"][0]);
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

#[tokio::test]
async fn observe_jobs_missing_baseline_is_immediate_and_projects_activity_without_item_wait() {
    let runtime = test_runtime();
    let (job_id, request, auth) = register_and_start_agent_job(&runtime, "observe-immediate").await;
    update_observed_job(
        &runtime,
        "observe-immediate",
        &request,
        "running",
        None,
        Some(process_activity()),
        false,
    )
    .await;

    let result = runtime
        .dispatch_with_auth(
            ToolCall::ObserveJobs {
                items: vec![item(&job_id, None)],
                tail_lines: 40,
                wait_secs: Some(60),
            },
            Some(&auth),
        )
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["wait"]["outcome"], "immediate");
    assert_eq!(result.output["wait"]["waited_ms"], 0);
    assert_eq!(result.output["items"][0]["output"]["status"], "running");
    assert_eq!(
        result.output["items"][0]["output"]["activity"],
        serde_json::to_value(process_activity()).unwrap()
    );
    assert_item_has_no_wait_metadata(&result.output["items"][0]);
}

#[tokio::test]
async fn observe_jobs_timeout_waits_once_for_multiple_active_jobs() {
    let runtime = test_runtime();
    let (job_a, request_a, auth) =
        register_and_start_agent_job(&runtime, "observe-timeout-a").await;
    let (job_b, request_b, _) = register_and_start_agent_job(&runtime, "observe-timeout-b").await;
    let (job_c, request_c, _) = register_and_start_agent_job(&runtime, "observe-timeout-c").await;
    for (client, request) in [
        ("observe-timeout-a", &request_a),
        ("observe-timeout-b", &request_b),
        ("observe-timeout-c", &request_c),
    ] {
        update_observed_job(
            &runtime,
            client,
            request,
            "running",
            None,
            Some(process_activity()),
            false,
        )
        .await;
    }
    let token_a = observation_token(&runtime, &job_a, &auth).await;
    let token_b = observation_token(&runtime, &job_b, &auth).await;
    let token_c = observation_token(&runtime, &job_c, &auth).await;

    let started = Instant::now();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ObserveJobs {
                items: vec![
                    item(&job_a, Some(token_a)),
                    item(&job_b, Some(token_b)),
                    item(&job_c, Some(token_c)),
                ],
                tail_lines: 40,
                wait_secs: Some(1),
            },
            Some(&auth),
        )
        .await;
    let elapsed = started.elapsed();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["wait"]["outcome"], "timeout");
    assert!(result.output["wait"]["waited_ms"].as_u64().unwrap() > 0);
    assert!(
        elapsed < Duration::from_millis(2500),
        "three Jobs must share one ~1s wait instead of multiplying it: {elapsed:?}"
    );
    assert_eq!(result.output["items"].as_array().unwrap().len(), 3);
    for item in result.output["items"].as_array().unwrap() {
        assert_eq!(item["output"]["status"], "running");
        assert_item_has_no_wait_metadata(item);
    }
}

#[tokio::test]
async fn observe_jobs_one_item_update_wakes_shared_wait_and_refreshes_all_snapshots() {
    let runtime = test_runtime();
    let (job_a, request_a, auth) = register_and_start_agent_job(&runtime, "observe-update-a").await;
    let (job_b, request_b, _) = register_and_start_agent_job(&runtime, "observe-update-b").await;
    update_observed_job(
        &runtime,
        "observe-update-a",
        &request_a,
        "running",
        None,
        Some(process_activity()),
        false,
    )
    .await;
    update_observed_job(
        &runtime,
        "observe-update-b",
        &request_b,
        "running",
        None,
        Some(process_activity()),
        false,
    )
    .await;
    let token_a = observation_token(&runtime, &job_a, &auth).await;
    let token_b = observation_token(&runtime, &job_b, &auth).await;

    let waiting_runtime = runtime.clone();
    let waiting_auth = auth.clone();
    let waiting_a = job_a.clone();
    let waiting_b = job_b.clone();
    let task = tokio::spawn(async move {
        waiting_runtime
            .dispatch_with_auth(
                ToolCall::ObserveJobs {
                    items: vec![
                        item(&waiting_a, Some(token_a)),
                        item(&waiting_b, Some(token_b)),
                    ],
                    tail_lines: 40,
                    wait_secs: Some(5),
                },
                Some(&waiting_auth),
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(75)).await;
    update_observed_job(
        &runtime,
        "observe-update-b",
        &request_b,
        "running",
        Some("second changed\n"),
        Some(process_activity()),
        false,
    )
    .await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["wait"]["outcome"], "updated");
    assert!(result.output["wait"]["waited_ms"].as_u64().unwrap() < 5_000);
    assert_eq!(result.output["items"][0]["output"]["status"], "running");
    assert_eq!(result.output["items"][1]["output"]["status"], "running");
    assert_eq!(result.output["items"][1]["output"]["changed"], true);
    assert!(result.output["items"][1]["output"]["stdout_tail"]
        .as_str()
        .unwrap()
        .contains("second changed"));
    for item in result.output["items"].as_array().unwrap() {
        assert_item_has_no_wait_metadata(item);
    }
}

#[tokio::test]
async fn observe_jobs_terminal_transition_wakes_shared_wait() {
    let runtime = test_runtime();
    let (job_id, request, auth) = register_and_start_agent_job(&runtime, "observe-terminal").await;
    update_observed_job(
        &runtime,
        "observe-terminal",
        &request,
        "running",
        None,
        Some(process_activity()),
        false,
    )
    .await;
    let token = observation_token(&runtime, &job_id, &auth).await;

    let waiting_runtime = runtime.clone();
    let waiting_auth = auth.clone();
    let waiting_job = job_id.clone();
    let task = tokio::spawn(async move {
        waiting_runtime
            .dispatch_with_auth(
                ToolCall::ObserveJobs {
                    items: vec![item(&waiting_job, Some(token))],
                    tail_lines: 40,
                    wait_secs: Some(5),
                },
                Some(&waiting_auth),
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(75)).await;
    update_observed_job(
        &runtime,
        "observe-terminal",
        &request,
        "completed",
        None,
        None,
        true,
    )
    .await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["wait"]["outcome"], "terminal");
    assert_eq!(result.output["terminal_count"], 1);
    assert_eq!(result.output["items"][0]["output"]["terminal"], true);
    assert!(result.output["items"][0]["output"]["activity"].is_null());
    assert_item_has_no_wait_metadata(&result.output["items"][0]);
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

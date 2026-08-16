//! Phase D bounded batch Job observation.

use super::super::*;
use super::support::*;
use crate::shell_protocol::{
    ShellAgentJobUpdateRequest, ShellClientCapabilities, ShellClientRegisterRequest,
    ShellCommandExecutionState, ShellJobInventory,
};
use serde_json::json;
use std::time::{Duration, Instant};

fn item(job_id: &str, token: Option<String>) -> ObserveJobsItem {
    ObserveJobsItem {
        job_id: job_id.to_string(),
        after_observation_token: token,
    }
}

async fn seed_local_job(
    runtime: &ToolRuntime,
    root: &std::path::Path,
    job_id: &str,
    status: &str,
    stdout: &str,
    stderr: &str,
) -> (LocalJobRecord, String) {
    let now = chrono::Utc::now().timestamp();
    let dir = write_fake_job(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        status,
        stdout,
        stderr,
        json!({
            "started_at": now,
            "max_runtime_secs": 3600,
            "purpose": "diagnostic",
            "cwd": ".",
            "shell": "bash",
        }),
    );
    if matches!(
        status,
        "completed" | "failed" | "stopped" | "lost" | "timeout" | "timed_out" | "cancelled"
    ) {
        std::fs::write(
            dir.join("exit_code"),
            if status == "completed" { "0" } else { "1" },
        )
        .unwrap();
        std::fs::write(dir.join("finished_at"), now.to_string()).unwrap();
    }
    let (record, _) = LocalJobRecord::initialize("demo".to_string(), dir).unwrap();
    let token = record.observe().unwrap().token(job_id).unwrap();
    runtime
        .local_jobs
        .lock()
        .await
        .insert(job_id.to_string(), record.clone());
    (record, token)
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
    let request = next_patch_agent_request(runtime, client_id)
        .await
        .expect("Agent Job start request");
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
    let request = next_agent_request_for_client(runtime, client_id)
        .await
        .expect("owned Agent Job request");
    assert_eq!(request.job_id.as_deref(), Some(job_id.as_str()));
    job_id
}

async fn update_agent_job(
    runtime: &ToolRuntime,
    client_id: &str,
    request: &crate::shell_protocol::ShellAgentShellRequest,
    status: &str,
    stdout: Option<&str>,
    state: Option<ShellCommandExecutionState>,
) {
    runtime
        .shell_clients
        .update_job(ShellAgentJobUpdateRequest {
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            job_id: request.job_id.clone().unwrap(),
            request_id: Some(request.request_id.clone()),
            update_seq: None,
            status: status.to_string(),
            stdout_chunk: stdout.map(str::to_string),
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code: state.map(|_| 0),
            duration_ms: state.map(|_| 10),
            error: None,
            command_execution_state: state,
            validation_progress: None,
            finished: state.is_some(),
        })
        .await
        .unwrap();
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

    let definition = super::super::tool_definition::lookup_tool_definition("observe_jobs").unwrap();
    assert!(definition.visibility.is_model_visible());
    assert_eq!(definition.category, "job");
    assert_eq!(
        definition.metadata.oauth_scope,
        Some(crate::auth::SCOPE_RUNTIME_READ)
    );
    assert!(definition.metadata.read_only);
    let manifest = super::super::surface::registered_tool_categories();
    assert!(manifest["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|name| name == "observe_jobs"));

    let opaque = "wjob1:l:job:private_epoch_body:7";
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
async fn observe_jobs_immediate_mixed_local_and_agent_preserves_order_and_projection() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let (local_record, _) = seed_local_job(
        &runtime,
        temp.path(),
        "local-mixed",
        "running",
        "local\n",
        "",
    )
    .await;
    let (agent_job, request, auth) =
        register_and_start_agent_job(&runtime, "observe-mixed-agent").await;
    update_agent_job(
        &runtime,
        "observe-mixed-agent",
        &request,
        "running",
        Some("agent\n"),
        None,
    )
    .await;

    let result = runtime
        .dispatch_with_auth(
            ToolCall::ObserveJobs {
                items: vec![item(&agent_job, None), item("local-mixed", None)],
                tail_lines: 40,
                wait_secs: Some(10),
            },
            Some(&auth),
        )
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["wake_reason"], "immediate");
    assert_eq!(result.output["waited_ms"], 0);
    assert_eq!(result.output["items"][0]["job_id"], agent_job);
    assert_eq!(result.output["items"][0]["output"]["executor"], "agent");
    assert_eq!(
        result.output["items"][0]["output"]["stdout_tail"],
        "agent\n"
    );
    assert_eq!(result.output["items"][1]["job_id"], "local-mixed");
    assert_eq!(result.output["items"][1]["output"]["executor"], "local");
    assert_eq!(result.output["items"][1]["output"]["stdout_tail"], "local");

    let canonical_local = runtime
        .job_log_for_auth(
            "local-mixed".to_string(),
            None,
            Some(40),
            Some(&auth),
            None,
            None,
        )
        .await;
    assert!(canonical_local.success);
    for field in [
        "status",
        "executor",
        "stdout_tail",
        "stderr_tail",
        "observation_token",
        "terminal",
        "validation",
    ] {
        assert_eq!(
            result.output["items"][1]["output"][field], canonical_local.output[field],
            "{field}"
        );
    }
    assert_eq!(local_record.read_text("status").as_deref(), Some("running"));
}

#[tokio::test]
async fn observe_jobs_isolates_unknown_and_token_binding_failures() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let (_, token_one) =
        seed_local_job(&runtime, temp.path(), "token-one", "running", "", "").await;
    let (_, token_two) =
        seed_local_job(&runtime, temp.path(), "token-two", "running", "", "").await;
    let (_, _token_three) =
        seed_local_job(&runtime, temp.path(), "token-three", "running", "", "").await;
    let (_, token_four) =
        seed_local_job(&runtime, temp.path(), "token-four", "running", "", "").await;
    let wrong_executor = crate::job_observation::JobObservationToken::new(
        crate::job_observation::JobObservationExecutor::Agent,
        "token-three",
        "epoch",
        0,
    )
    .unwrap()
    .encode();

    let result = runtime
        .dispatch(ToolCall::ObserveJobs {
            items: vec![
                item("token-one", Some(token_one)),
                item("token-two", Some(token_four)),
                item("token-three", Some(wrong_executor)),
                item("token-four", Some("malformed".to_string())),
                item("missing-job", Some(token_two)),
            ],
            tail_lines: 40,
            wait_secs: Some(5),
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["wake_reason"], "item_error");
    assert_eq!(result.output["succeeded_count"], 1);
    assert_eq!(result.output["failed_count"], 4);
    assert_eq!(result.output["items"][0]["success"], true);
    for index in 1..=3 {
        assert_eq!(
            result.output["items"][index]["error_kind"],
            "invalid_observation_token"
        );
        assert!(result.output["items"][index]["output"].is_null());
    }
    assert_eq!(result.output["items"][4]["error_kind"], "unknown_job");
    assert!(result.output["items"][4]["error"]
        .as_str()
        .unwrap()
        .contains("unknown job"));
    assert_eq!(result.output["returned_count"], 5);
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
async fn observe_jobs_old_epoch_token_refreshes_without_waiting() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    seed_local_job(&runtime, temp.path(), "epoch-job", "running", "", "").await;
    let stale = crate::job_observation::JobObservationToken::new(
        crate::job_observation::JobObservationExecutor::Local,
        "epoch-job",
        "old-server-epoch",
        0,
    )
    .unwrap()
    .encode();
    let started = Instant::now();
    let result = runtime
        .dispatch(ToolCall::ObserveJobs {
            items: vec![item("epoch-job", Some(stale))],
            tail_lines: 40,
            wait_secs: Some(5),
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["wake_reason"], "updated");
    assert_eq!(result.output["changed_count"], 1);
    assert_eq!(result.output["waited_ms"], 0);
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[tokio::test]
async fn observe_jobs_agent_change_wakes_shared_wait_and_refreshes_all_items() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let (_, local_token) = seed_local_job(
        &runtime,
        temp.path(),
        "agent-wake-sibling",
        "running",
        "stable\n",
        "",
    )
    .await;
    let (agent_job, request, auth) =
        register_and_start_agent_job(&runtime, "observe-agent-wake").await;
    update_agent_job(
        &runtime,
        "observe-agent-wake",
        &request,
        "running",
        None,
        None,
    )
    .await;
    let initial_agent = runtime
        .job_log_for_auth(agent_job.clone(), None, Some(40), Some(&auth), None, None)
        .await;
    let agent_token = initial_agent.output["observation_token"]
        .as_str()
        .unwrap()
        .to_string();

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let agent_job = agent_job.clone();
        async move {
            runtime
                .observe_jobs_for_auth(
                    vec![
                        item("agent-wake-sibling", Some(local_token)),
                        item(&agent_job, Some(agent_token)),
                    ],
                    40,
                    Some(5),
                    Some(&auth),
                )
                .await
        }
    });
    tokio::time::sleep(Duration::from_millis(150)).await;
    update_agent_job(
        &runtime,
        "observe-agent-wake",
        &request,
        "running",
        Some("agent changed\n"),
        None,
    )
    .await;
    let result = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .expect("batch should wake after Agent change")
        .unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["wake_reason"], "updated");
    assert_eq!(result.output["items"][0]["output"]["stdout_tail"], "stable");
    assert!(result.output["items"][1]["output"]["stdout_tail"]
        .as_str()
        .unwrap()
        .contains("agent changed"));
    assert_eq!(result.output["changed_count"], 1);
    assert!(result.output["waited_ms"].as_u64().unwrap() < 2_000);
}

#[tokio::test]
async fn observe_jobs_local_change_wakes_and_dropped_sibling_wait_is_read_only() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let (changed_record, changed_token) =
        seed_local_job(&runtime, temp.path(), "local-wake", "running", "", "").await;
    let (sibling_record, sibling_token) =
        seed_local_job(&runtime, temp.path(), "local-sibling", "running", "", "").await;
    let sibling_before = sibling_record.observe().unwrap();

    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .observe_jobs_for_auth(
                    vec![
                        item("local-wake", Some(changed_token)),
                        item("local-sibling", Some(sibling_token)),
                    ],
                    40,
                    Some(5),
                    None,
                )
                .await
        }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    std::fs::write(changed_record.dir.join("stdout.log"), "local changed\n").unwrap();
    let result = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("batch should wake after local change")
        .unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["wake_reason"], "updated");
    assert_eq!(
        result.output["items"][0]["output"]["stdout_tail"],
        "local changed"
    );
    assert_eq!(result.output["items"][1]["output"]["status"], "running");
    let sibling_after = sibling_record.observe().unwrap();
    assert_eq!(sibling_after.status, sibling_before.status);
    assert_eq!(sibling_after.revision, sibling_before.revision);
    assert_eq!(
        sibling_record.read_text("status").as_deref(),
        Some("running")
    );
}

#[tokio::test]
async fn observe_jobs_terminal_state_and_transition_never_wait_pointlessly() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let (_, terminal_token) = seed_local_job(
        &runtime,
        temp.path(),
        "already-terminal",
        "completed",
        "done\n",
        "",
    )
    .await;
    let started = Instant::now();
    let terminal = runtime
        .observe_jobs_for_auth(
            vec![item("already-terminal", Some(terminal_token))],
            40,
            Some(5),
            None,
        )
        .await;
    assert!(terminal.success);
    assert_eq!(terminal.output["wake_reason"], "terminal");
    assert_eq!(terminal.output["terminal_count"], 1);
    assert_eq!(terminal.output["waited_ms"], 0);
    assert!(started.elapsed() < Duration::from_secs(1));

    let (transition_record, transition_token) = seed_local_job(
        &runtime,
        temp.path(),
        "terminal-transition",
        "running",
        "",
        "",
    )
    .await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .observe_jobs_for_auth(
                    vec![item("terminal-transition", Some(transition_token))],
                    40,
                    Some(5),
                    None,
                )
                .await
        }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    std::fs::write(transition_record.dir.join("exit_code"), "0").unwrap();
    std::fs::write(
        transition_record.dir.join("finished_at"),
        chrono::Utc::now().timestamp().to_string(),
    )
    .unwrap();
    std::fs::write(transition_record.dir.join("status"), "completed").unwrap();
    let transitioned = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("terminal transition should wake")
        .unwrap();
    assert_eq!(transitioned.output["wake_reason"], "terminal");
    assert_eq!(
        transitioned.output["items"][0]["output"]["status"],
        "completed"
    );
    assert_eq!(transitioned.output["terminal_count"], 1);
}

#[tokio::test]
async fn observe_jobs_recovering_lost_and_stop_requested_match_job_log_semantics() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    seed_local_job(&runtime, temp.path(), "lost-local", "lost", "", "").await;
    let lost_log = runtime
        .job_log_for_auth("lost-local".to_string(), None, Some(40), None, None, None)
        .await;
    let lost_batch = runtime
        .observe_jobs_for_auth(vec![item("lost-local", None)], 40, None, None)
        .await;
    assert_eq!(
        lost_batch.output["items"][0]["output"]["status"],
        lost_log.output["status"]
    );
    assert_eq!(
        lost_batch.output["items"][0]["output"]["terminal"],
        lost_log.output["terminal"]
    );

    let recovering_caps = ShellClientCapabilities {
        async_jobs: true,
        async_shell_jobs: true,
        job_state_reconciliation: true,
        ..Default::default()
    };
    runtime
        .shell_clients
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: Some(ShellJobInventory {
                active_complete: true,
                jobs: Vec::new(),
            }),
            client_id: "observe-recovering".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: Some(recovering_caps),
            projects: Some(vec![registered_project(
                "agent-proj",
                "/tmp/observe-recovering",
            )]),
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap();
    let auth = bootstrap_auth_context();
    let started = runtime
        .dispatch_with_auth(
            ToolCall::RunJob {
                project: agent_test_project_id("observe-recovering"),
                command: "sleep 30".to_string(),
                session_id: None,
                timeout_secs: Some(60),
                cwd: None,
                purpose: None,
                shell: None,
            },
            Some(&auth),
        )
        .await;
    let recovering_job = started.output["job_id"].as_str().unwrap().to_string();
    let recovering_request = next_patch_agent_request(&runtime, "observe-recovering")
        .await
        .unwrap();
    runtime
        .shell_clients
        .update_job(ShellAgentJobUpdateRequest {
            client_id: "observe-recovering".to_string(),
            agent_instance_id: "inst".to_string(),
            job_id: recovering_job.clone(),
            request_id: Some(recovering_request.request_id),
            update_seq: Some(1),
            status: "running".to_string(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            command_execution_state: None,
            validation_progress: None,
            finished: false,
        })
        .await
        .unwrap();
    runtime
        .shell_clients
        .reconcile_disconnect("observe-recovering", "inst")
        .await;
    let recovering_log = runtime
        .job_log_for_auth(
            recovering_job.clone(),
            None,
            Some(40),
            Some(&auth),
            None,
            None,
        )
        .await;
    assert_eq!(recovering_log.output["status"], "recovering");
    let recovering_batch = runtime
        .observe_jobs_for_auth(vec![item(&recovering_job, None)], 40, None, Some(&auth))
        .await;
    for field in [
        "status",
        "terminal",
        "recovery_state",
        "recovery_reason_code",
        "recovery_reason",
    ] {
        assert_eq!(
            recovering_batch.output["items"][0]["output"][field], recovering_log.output[field],
            "{field}"
        );
    }

    let (stop_job, stop_request, stop_auth) =
        register_and_start_agent_job(&runtime, "observe-stop-requested").await;
    update_agent_job(
        &runtime,
        "observe-stop-requested",
        &stop_request,
        "running",
        None,
        None,
    )
    .await;
    let stopped = runtime
        .dispatch_with_auth(
            ToolCall::StopJob {
                project: agent_test_project_id("observe-stop-requested"),
                job_id: stop_job.clone(),
                session_id: None,
                confirm: true,
            },
            Some(&stop_auth),
        )
        .await;
    assert!(stopped.success, "{:?}", stopped.error);
    let stop_log = runtime
        .job_log_for_auth(
            stop_job.clone(),
            None,
            Some(40),
            Some(&stop_auth),
            None,
            None,
        )
        .await;
    assert_eq!(stop_log.output["status"], "stop_requested");
    let stop_batch = runtime
        .observe_jobs_for_auth(vec![item(&stop_job, None)], 40, None, Some(&stop_auth))
        .await;
    assert_eq!(
        stop_batch.output["items"][0]["output"]["status"],
        stop_log.output["status"]
    );
    assert_eq!(
        stop_batch.output["items"][0]["output"]["terminal"],
        stop_log.output["terminal"]
    );
}

#[tokio::test]
async fn observe_jobs_eight_unchanged_jobs_use_one_wait_budget() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let mut items = Vec::new();
    for index in 0..8 {
        let job_id = format!("unchanged-{index}");
        let (_, token) = seed_local_job(&runtime, temp.path(), &job_id, "running", "", "").await;
        items.push(item(&job_id, Some(token)));
    }
    let started = Instant::now();
    let result = runtime
        .observe_jobs_for_auth(items, 40, Some(1), None)
        .await;
    let elapsed = started.elapsed();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["wake_reason"], "timeout");
    assert!(
        elapsed < Duration::from_secs(3),
        "one-second batch wait serialized across items: {elapsed:?}"
    );
    assert!(
        elapsed >= Duration::from_millis(800),
        "batch returned before its one shared deadline: {elapsed:?}"
    );
    assert_eq!(result.output["changed_count"], 0);
    assert_eq!(result.output["terminal_count"], 0);
}

#[tokio::test]
async fn observe_jobs_output_budget_keeps_whole_items_and_continuation_index() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let log = (0..200)
        .map(|index| format!("{index:03}:{}", "x".repeat(235)))
        .collect::<Vec<_>>()
        .join("\n");
    let mut items = Vec::new();
    for index in 0..4 {
        let job_id = format!("budget-{index}");
        seed_local_job(&runtime, temp.path(), &job_id, "running", &log, &log).await;
        items.push(item(&job_id, None));
    }
    let result = runtime.observe_jobs_for_auth(items, 200, None, None).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["output_truncated"], true);
    let returned = result.output["returned_count"].as_u64().unwrap() as usize;
    assert!(returned > 0 && returned < 4);
    assert_eq!(result.output["next_index"], returned);
    assert_eq!(result.output["items"].as_array().unwrap().len(), returned);
    assert!(
        serde_json::to_vec(&result).unwrap().len()
            <= webcodex_workspace::file_read_range::MAX_SERIALIZED_OUTPUT_BYTES
    );
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
    assert!(next_patch_agent_request(&runtime, "observe-no-enqueue")
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
    let opaque = "wjob1:l:job:opaque-private-body:123";
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

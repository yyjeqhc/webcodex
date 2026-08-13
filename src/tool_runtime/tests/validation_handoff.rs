//! Deterministic tests for structured validation auto-continuation as a Job.
//!
//! `cargo_check` / `cargo_test` / `cargo_fmt(check=true)` run the command
//! exactly once: short validations finish in-process and return the existing
//! terminal structure (no redundant visible Job); long validations promote the
//! same execution to a queryable Job and return a `job_id`. `cargo_fmt`
//! (mutating) never auto-promotes.

use super::support::*;
use crate::shell_protocol::{
    ShellAgentJobUpdateRequest, ShellAgentResultPayload, ShellAgentResultRequest,
    ShellClientCapabilities, ShellCommandExecutionState, ShellJobValidationProgress,
};
use crate::tool_runtime::tool_inputs::ExecutionPurpose;
use crate::tool_runtime::validation_events::validation_summary_for_session;
use crate::tool_runtime::validation_profile::{
    validation_adapter_for_tool, ValidationCommandOptions,
};
use crate::tool_runtime::{ObserveJobsItem, ToolCall, ToolRuntime};
use serde_json::json;

/// Fetch the `start_validation_job` request that the agent should have polled
/// and return the job id embedded in it.
async fn poll_start_validation_job(
    runtime: &ToolRuntime,
    client_id: &str,
) -> (crate::shell_protocol::ShellAgentShellRequest, String) {
    let request = next_patch_agent_request(runtime, client_id)
        .await
        .expect("structured validation should enqueue a start_validation_job request");
    assert_eq!(request.kind, "start_validation_job", "{:?}", request.kind);
    let job_id = request.job_id.clone().expect("start_validation_job job_id");
    (request, job_id)
}

async fn wait_for_agent_request(
    runtime: &ToolRuntime,
    client_id: &str,
) -> crate::shell_protocol::ShellAgentShellRequest {
    for _ in 0..200 {
        if let Some(request) = next_patch_agent_request(runtime, client_id).await {
            return request;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("agent request was not enqueued for {client_id}");
}

async fn complete_sync_shell_lifecycle(
    runtime: &ToolRuntime,
    client_id: &str,
    request_id: String,
    execution_state: ShellCommandExecutionState,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
    error: Option<&str>,
) {
    runtime
        .shell_clients
        .complete(ShellAgentResultPayload {
            result: ShellAgentResultRequest {
                client_id: client_id.to_string(),
                agent_instance_id: "inst".to_string(),
                request_id,
                exit_code,
                stdout: Some(stdout.to_string()),
                stderr: Some(stderr.to_string()),
                duration_ms: Some(5),
                error: error.map(str::to_string),
            },
            command_execution_state: Some(execution_state),
        })
        .await
        .unwrap();
}

fn cargo_test_update(
    client_id: &str,
    request_id: &str,
    job_id: &str,
    status: &str,
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
    progress: ShellJobValidationProgress,
    finished: bool,
) -> ShellAgentJobUpdateRequest {
    ShellAgentJobUpdateRequest {
        client_id: client_id.to_string(),
        agent_instance_id: "inst".to_string(),
        update_seq: None,
        job_id: job_id.to_string(),
        request_id: Some(request_id.to_string()),
        status: status.to_string(),
        stdout_chunk: None,
        stderr_chunk: None,
        stdout_tail: Some(stdout.to_string()),
        stderr_tail: Some(stderr.to_string()),
        log_snapshot: None,
        exit_code,
        duration_ms: Some(25),
        error: None,
        command_execution_state: None,
        validation_progress: Some(progress),
        finished,
    }
}

fn running_progress(step: &str) -> ShellJobValidationProgress {
    ShellJobValidationProgress {
        completed: 0,
        current_step: Some(step.to_string()),
        failed_step: None,
    }
}

fn completed_progress() -> ShellJobValidationProgress {
    ShellJobValidationProgress {
        completed: 1,
        current_step: None,
        failed_step: None,
    }
}

fn assert_cargo_result_matches_schema(tool_name: &str, result: &crate::tool_runtime::ToolResult) {
    use crate::tool_runtime::registry::output_schema_for_tool;
    use crate::tool_runtime::startup_brief::validate_schema_instance_for_test;

    let schema = output_schema_for_tool(tool_name);
    let value = serde_json::to_value(result).unwrap();
    assert!(
        validate_schema_instance_for_test(&value, &schema).is_ok(),
        "{tool_name} result did not satisfy its output schema: {value}"
    );
}

fn write_local_validation_crate(root: &std::path::Path, source: &str) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"local-validation-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(root.join("src/lib.rs"), source).unwrap();
}

async fn wait_for_local_job_terminal(
    runtime: &ToolRuntime,
    job_id: &str,
) -> crate::tool_runtime::ToolResult {
    for _ in 0..500 {
        let status = runtime.job_status(job_id.to_string()).await;
        if status.success && status.output["terminal"].as_bool().unwrap_or(false) {
            return status;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("local validation job did not become terminal: {job_id}");
}

#[tokio::test]
async fn go_test_fails_closed_without_structured_go_capability() {
    let client_id = "vhandoff-go-capability";
    let runtime = runtime_with_agent_project(client_id)
        .with_validation_sync_wait(std::time::Duration::from_millis(20));
    register_agent(
        &runtime,
        client_id,
        None,
        ShellClientCapabilities {
            async_shell_jobs: true,
            structured_validation_argv: true,
            structured_go_test_json: false,
            ..Default::default()
        },
    )
    .await;

    let result = runtime
        .go_test(agent_test_project_id(client_id), None, Some(1800))
        .await;

    assert!(!result.success);
    assert_eq!(result.output["failure_kind"], "capability_unavailable");
    assert_eq!(result.output["command_started"], false);
    assert!(result
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("structured_go_test_json"));
    assert!(next_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
}

#[tokio::test]
async fn go_test_requires_first_class_tool_capability_before_job_reservation() {
    let client_id = "vhandoff-go-old-runner";
    let runtime = runtime_with_agent_project(client_id)
        .with_validation_sync_wait(std::time::Duration::from_millis(20));
    register_agent(
        &runtime,
        client_id,
        None,
        ShellClientCapabilities {
            async_shell_jobs: true,
            structured_validation_argv: true,
            structured_go_test_json: true,
            structured_go_test_tool: false,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client_id);
    let auth = auth_context(None, true);
    let session = runtime.sessions.start_session(Some(project.clone()), None);

    let result = runtime
        .dispatch_with_auth(
            ToolCall::GoTest {
                project: project.clone(),
                session_id: Some(session.session_id.clone()),
                cwd: None,
                timeout_secs: Some(1800),
            },
            Some(&auth),
        )
        .await;

    assert!(!result.success);
    assert_eq!(result.output["failure_kind"], "capability_unavailable");
    assert_eq!(result.output["command_started"], false);
    assert!(result
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("structured_go_test_tool"));
    assert!(runtime.shell_clients.list_jobs(Some(10)).await.is_empty());
    assert!(next_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(50))
        .unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["status"], "not_run");
    assert_eq!(validation["events_total"], 0);
    assert!(validation["latest"].is_null());

    // The new first-class bit must not narrow unrelated structured validation
    // on an otherwise capable older Runner.
    let check_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::CargoCheck {
                        project,
                        session_id: None,
                        cwd: None,
                        all_targets: None,
                        all_features: None,
                        no_default_features: None,
                        features: None,
                        package: None,
                        timeout_secs: Some(600),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let (request, job_id) = poll_start_validation_job(&runtime, client_id).await;
    let steps: Vec<crate::shell_protocol::ShellJobValidationStep> =
        serde_json::from_str(&request.command).unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].name, "check");
    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "completed",
            "Finished `dev` profile [unoptimized + debuginfo] target(s)\n",
            "",
            Some(0),
            completed_progress(),
            true,
        ))
        .await
        .unwrap();
    let check = check_task.await.unwrap();
    assert!(check.success, "{:?}", check.error);
}

#[tokio::test]
async fn fast_go_test_uses_exact_structured_argv_cwd_and_records_session_evidence() {
    let client_id = "vhandoff-go-fast";
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("project");
    let scoped = root.join("internal/nodeapp");
    std::fs::create_dir_all(&scoped).unwrap();
    let runtime = test_runtime().with_validation_sync_wait(std::time::Duration::from_millis(300));
    register_agent_with_projects(
        &runtime,
        client_id,
        None,
        ShellClientCapabilities {
            async_shell_jobs: true,
            structured_validation_argv: true,
            structured_go_test_json: true,
            structured_go_test_tool: true,
            ..Default::default()
        },
        vec![registered_project(
            "go-demo",
            root.to_string_lossy().as_ref(),
        )],
    )
    .await;
    let project = crate::tool_runtime::agent_project_runtime_id(client_id, "go-demo");
    let auth = auth_context(None, true);
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let session_id = session.session_id.clone();

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        let session_id = session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::GoTest {
                        project,
                        session_id: Some(session_id),
                        cwd: Some("internal/nodeapp".to_string()),
                        timeout_secs: Some(1800),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = wait_for_agent_request(&runtime, client_id).await;
    assert_eq!(request.kind, "start_validation_job");
    let job_id = request.job_id.clone().expect("start_validation_job job_id");
    assert_eq!(
        request.cwd.as_deref(),
        Some(scoped.to_string_lossy().as_ref())
    );
    let steps: serde_json::Value = serde_json::from_str(&request.command).unwrap();
    assert_eq!(steps.as_array().unwrap().len(), 1);
    assert_eq!(steps[0]["name"], "test");
    assert_eq!(steps[0]["program"], "go");
    assert_eq!(steps[0]["args"], json!(["test", "-json", "./..."]));
    assert!(steps[0].get("env").is_none());

    let stdout = concat!(
        "{\"Action\":\"run\",\"Package\":\"example/pkg\",\"Test\":\"TestPass\"}\n",
        "{\"Action\":\"pass\",\"Package\":\"example/pkg\",\"Test\":\"TestPass\",\"Elapsed\":0}\n",
        "{\"Action\":\"run\",\"Package\":\"example/pkg\",\"Test\":\"TestSkip\"}\n",
        "{\"Action\":\"skip\",\"Package\":\"example/pkg\",\"Test\":\"TestSkip\",\"Elapsed\":0}\n",
        "{\"Action\":\"pass\",\"Package\":\"example/pkg\",\"Elapsed\":0}\n"
    );
    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "completed",
            stdout,
            "",
            Some(0),
            completed_progress(),
            true,
        ))
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["command_summary"], "go test -json ./...");
    assert_eq!(result.output["cwd"], "internal/nodeapp");
    assert_eq!(result.output["promoted_to_job"], false);
    assert_eq!(result.output["tests_detected"], true);
    assert_eq!(result.output["tests_run_count"], 2);
    assert_eq!(result.output["tests_passed"], 1);
    assert_eq!(result.output["tests_failed"], 0);
    assert_eq!(result.output["diagnostics"]["test_summary"]["ignored"], 1);
    assert_cargo_result_matches_schema("go_test", &result);

    let session = runtime.sessions.summary(&session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);
    assert_eq!(validation["status"], "passed");
    assert_eq!(validation["latest"]["tool_name"], "go_test");
    assert_eq!(validation["latest"]["validation_kind"], "test");
    assert!(validation["latest"]["identity"]
        .as_str()
        .unwrap_or_default()
        .starts_with("command:"));
    assert_eq!(validation["latest"]["tests_run_count"], 2);
    assert_eq!(
        validation["latest"]["diagnostics"]["test_summary"]["ignored"],
        1
    );
    let serialized = serde_json::to_string(&validation).unwrap();
    assert!(!serialized.contains(root.to_string_lossy().as_ref()));
}

#[tokio::test]
async fn go_test_failure_reports_failed_test_identity_in_result_and_session() {
    let client_id = "vhandoff-go-failure";
    let runtime = runtime_with_agent_project(client_id)
        .with_validation_sync_wait(std::time::Duration::from_millis(300));
    register_agent(
        &runtime,
        client_id,
        None,
        ShellClientCapabilities {
            async_shell_jobs: true,
            structured_validation_argv: true,
            structured_go_test_json: true,
            structured_go_test_tool: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client_id);
    let auth = auth_context(None, true);
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let session_id = session.session_id.clone();

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let session_id = session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::GoTest {
                        project,
                        session_id: Some(session_id),
                        cwd: None,
                        timeout_secs: Some(1800),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = wait_for_agent_request(&runtime, client_id).await;
    assert_eq!(request.kind, "start_validation_job");
    let job_id = request.job_id.clone().expect("start_validation_job job_id");
    let stdout = concat!(
        "{\"Action\":\"run\",\"Package\":\"example/pkg\",\"Test\":\"TestFail\"}\n",
        "{\"Action\":\"fail\",\"Package\":\"example/pkg\",\"Test\":\"TestFail\",\"Elapsed\":0}\n",
        "{\"Action\":\"fail\",\"Package\":\"example/pkg\",\"Elapsed\":0}\n"
    );
    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "failed",
            stdout,
            "",
            Some(1),
            completed_progress(),
            true,
        ))
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["failure_kind"], "validation_failed");
    assert_eq!(result.output["tests_failed"], 1);
    assert_eq!(
        result.output["diagnostics"]["failed_test_details"][0]["name"],
        "example/pkg::TestFail"
    );
    assert_cargo_result_matches_schema("go_test", &result);

    let session = runtime.sessions.summary(&session_id, Some(50)).unwrap();
    let validation = validation_summary_for_session(&session);
    assert_eq!(validation["status"], "failed");
    assert_eq!(validation["latest"]["tool_name"], "go_test");
    assert_eq!(validation["latest"]["failure_kind"], "test_failure");
    assert_eq!(
        validation["latest"]["diagnostics"]["failed_test_details"][0]["name"],
        "example/pkg::TestFail"
    );
}

#[tokio::test]
async fn long_go_test_hands_off_same_job_and_terminal_evidence_is_queryable() {
    let client_id = "vhandoff-go-long";
    let runtime = runtime_with_agent_project(client_id)
        .with_validation_sync_wait(std::time::Duration::from_millis(20));
    register_agent(
        &runtime,
        client_id,
        None,
        ShellClientCapabilities {
            async_shell_jobs: true,
            structured_validation_argv: true,
            structured_go_test_json: true,
            structured_go_test_tool: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client_id);
    let auth = auth_context(None, true);
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let session_id = session.session_id.clone();

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        let session_id = session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::GoTest {
                        project,
                        session_id: Some(session_id),
                        cwd: None,
                        timeout_secs: Some(1800),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = wait_for_agent_request(&runtime, client_id).await;
    assert_eq!(request.kind, "start_validation_job");
    let job_id = request.job_id.clone().expect("start_validation_job job_id");
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["promoted_to_job"], true);
    assert_eq!(result.output["terminal"], false);
    assert_eq!(result.output["job_id"], job_id);
    let observation_token = result.output["observation_token"]
        .as_str()
        .expect("go_test handoff observation token")
        .to_string();
    let observed = runtime
        .observe_jobs_for_auth(
            vec![ObserveJobsItem {
                job_id: job_id.clone(),
                after_observation_token: Some(observation_token.clone()),
            }],
            40,
            None,
            Some(&auth),
        )
        .await;
    assert!(observed.success, "{:?}", observed.error);
    assert_eq!(observed.output["items"][0]["success"], true);
    assert_eq!(
        observed.output["items"][0]["output"]["observation_token"],
        observation_token
    );
    assert_cargo_result_matches_schema("go_test", &result);

    let stdout = concat!(
        "{\"Action\":\"run\",\"Package\":\"example/pkg\",\"Test\":\"TestLater\"}\n",
        "{\"Action\":\"pass\",\"Package\":\"example/pkg\",\"Test\":\"TestLater\",\"Elapsed\":0}\n",
        "{\"Action\":\"pass\",\"Package\":\"example/pkg\",\"Elapsed\":0}\n"
    );
    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "completed",
            stdout,
            "",
            Some(0),
            completed_progress(),
            true,
        ))
        .await
        .unwrap();

    let session = runtime.sessions.summary(&session_id, Some(50)).unwrap();
    let validation = runtime
        .validation_summary_for_session_with_jobs(&session, 20, Some(&auth))
        .await;
    assert_eq!(validation["status"], "passed");
    assert_eq!(validation["latest"]["tool_name"], "go_test");
    assert_eq!(validation["latest"]["tests_run_count"], 1);
    assert_eq!(
        validation["latest"]["diagnostics"]["test_summary"]["passed"],
        1
    );
}

/// Fast sync-complete: a validation that finishes within the tiny injected
/// sync window returns the existing terminal structure and leaves no visible
/// Job in `list_jobs`.
#[tokio::test]
async fn fast_cargo_check_completes_in_windows_and_leaves_no_visible_job() {
    let client_id = "vhandoff-fast-check";
    let runtime = runtime_with_agent_project(client_id)
        .with_validation_sync_wait(std::time::Duration::from_millis(300));
    let caps = ShellClientCapabilities {
        async_shell_jobs: true,
        structured_validation_argv: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);
    let auth = auth_context(None, true);
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let session_id = session.session_id.clone();

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let session_id = session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::CargoCheck {
                        project,
                        session_id: Some(session_id),
                        cwd: None,
                        all_targets: Some(true),
                        all_features: None,
                        no_default_features: None,
                        features: None,
                        package: None,
                        timeout_secs: Some(600),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let (request, job_id) = poll_start_validation_job(&runtime, client_id).await;
    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "completed",
            "Finished `dev` profile [unoptimized + debuginfo] target(s)\n",
            "",
            Some(0),
            completed_progress(),
            true,
        ))
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["execution_state"], "completed");
    assert_eq!(result.output["passed"], true);
    assert_eq!(result.output["command_completed"], true);
    assert_eq!(result.output["promoted_to_job"], false);
    assert_eq!(result.output["effective_timeout_secs"], 600);
    assert!(result.output.get("observation_token").is_none());
    assert_eq!(result.output["sync_wait_secs"], 90);
    assert_cargo_result_matches_schema("cargo_check", &result);
    // No redundant visible job.
    let list = runtime.list_jobs_for_auth(None, None, None).await;
    assert!(list.success);
    assert!(list.output["jobs"].as_array().unwrap().is_empty());

    // Session has exactly one validation evidence record (the terminal check).
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(50))
        .unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["events_total"], 1);
    assert_eq!(validation["status"], "passed");
    assert_eq!(validation["latest"]["tool_name"], "cargo_check");
}

#[tokio::test]
async fn long_cargo_check_hands_off_with_immediately_observable_token() {
    let client_id = "vhandoff-long-check";
    let runtime = runtime_with_agent_project(client_id)
        .with_validation_sync_wait(std::time::Duration::from_millis(20));
    register_agent(
        &runtime,
        client_id,
        None,
        ShellClientCapabilities {
            async_shell_jobs: true,
            structured_validation_argv: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client_id);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .cargo_check(project, None, None, None, None, None, None, Some(600))
                .await
        }
    });
    let (request, job_id) = poll_start_validation_job(&runtime, client_id).await;
    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "running",
            "Checking demo v0.1.0\n",
            "",
            None,
            running_progress("check"),
            false,
        ))
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["promoted_to_job"], true);
    assert_eq!(result.output["job_id"], job_id);
    let observation_token = result.output["observation_token"]
        .as_str()
        .expect("cargo_check handoff observation token")
        .to_string();
    let observed = runtime
        .observe_jobs_for_auth(
            vec![ObserveJobsItem {
                job_id: job_id.clone(),
                after_observation_token: Some(observation_token.clone()),
            }],
            40,
            None,
            None,
        )
        .await;
    assert!(observed.success, "{:?}", observed.error);
    assert_eq!(observed.output["items"][0]["success"], true);
    assert_eq!(
        observed.output["items"][0]["output"]["observation_token"],
        observation_token
    );
    assert_cargo_result_matches_schema("cargo_check", &result);
}

/// Auto handoff: a validation still running when the injected sync window
/// elapses returns a successful Job handoff with an immediately queryable
/// `job_id`, and the command continues running.
#[tokio::test]
async fn long_cargo_test_hands_off_to_queryable_job() {
    let client_id = "vhandoff-long-test";
    let runtime = runtime_with_agent_project(client_id)
        .with_validation_sync_wait(std::time::Duration::from_millis(50));
    let caps = ShellClientCapabilities {
        async_shell_jobs: true,
        structured_validation_argv: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .cargo_test(
                    project,
                    None,
                    Some("slow".to_string()),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(1800),
                )
                .await
        }
    });
    let (request, job_id) = poll_start_validation_job(&runtime, client_id).await;
    // Mark it running (the agent polls and starts executing).
    assert!(runtime.shell_clients.list_jobs(Some(10)).await.is_empty());
    assert_eq!(
        runtime
            .shell_clients
            .count_active_jobs_for_project(None, &project)
            .await,
        0,
        "hidden pre-handoff jobs must not enter public active counts"
    );
    // Mark it running (the agent polls and starts executing).
    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "running",
            "running 1 test\n",
            "",
            None,
            running_progress("test"),
            false,
        ))
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["promoted_to_job"], true);
    assert_eq!(result.output["execution_state"], "running");
    assert_eq!(result.output["job_status"], "running");
    assert_eq!(result.output["command_started"], true);
    assert_eq!(result.output["command_completed"], false);
    assert_eq!(result.output["effective_timeout_secs"], 1800);
    assert_eq!(result.output["sync_wait_secs"], 90);
    assert!(result.output.get("passed").is_none());
    assert!(result.output.get("failure_kind").is_none());
    assert_eq!(result.output["job_id"].as_str().unwrap(), job_id.as_str());
    assert_cargo_result_matches_schema("cargo_test", &result);
    let observation_token = result.output["observation_token"]
        .as_str()
        .expect("cargo_test handoff observation token")
        .to_string();
    let observed = runtime
        .observe_jobs_for_auth(
            vec![ObserveJobsItem {
                job_id: job_id.clone(),
                after_observation_token: Some(observation_token.clone()),
            }],
            40,
            None,
            None,
        )
        .await;
    assert!(observed.success, "{:?}", observed.error);
    assert_eq!(observed.output["items"][0]["success"], true);
    assert_eq!(
        observed.output["items"][0]["output"]["observation_token"],
        observation_token
    );

    // Job is immediately queryable and still active.
    let status = runtime
        .job_status_for_auth(job_id.clone(), false, None)
        .await;
    assert!(status.success);
    assert_eq!(status.output["status"], "running");
    assert_eq!(status.output["active"], true);
    assert_eq!(status.output["validation"]["tool"], "cargo_test");
    assert_eq!(status.output["validation"]["kind"], "test");
    assert_eq!(status.output["validation"]["state"], "running");
    let log = runtime
        .job_log_for_auth(job_id.clone(), None, Some(200), None, None, None)
        .await;
    assert!(log.success);
    assert!(log.output["stdout_tail"]
        .as_str()
        .unwrap_or("")
        .contains("running 1 test"));
    assert_eq!(log.output["validation"], status.output["validation"]);
    let observed = runtime
        .observe_jobs_for_auth(
            vec![ObserveJobsItem {
                job_id,
                after_observation_token: Some(observation_token),
            }],
            200,
            None,
            None,
        )
        .await;
    assert!(observed.success, "{:?}", observed.error);
    assert_eq!(
        observed.output["items"][0]["output"]["validation"],
        log.output["validation"]
    );
}

/// Exactly-once execution: a fake command appends to a counter file; the count
/// must be exactly 1 whether the validation completes synchronously or is
/// promoted to a Job.
#[tokio::test]
async fn validation_command_starts_exactly_once_across_handoff() {
    let client_id = "vhandoff-once";
    let tmp = tempfile::tempdir().unwrap();
    let counter = tmp.path().join("starts.txt");
    let runtime = runtime_with_agent_project(client_id)
        .with_validation_sync_wait(std::time::Duration::from_millis(50));
    let caps = ShellClientCapabilities {
        async_shell_jobs: true,
        structured_validation_argv: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .cargo_test(
                    project,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(3600),
                )
                .await
        }
    });
    let (request, job_id) = poll_start_validation_job(&runtime, client_id).await;
    // The runner "executes" the job once: append to the counter.
    std::fs::write(&counter, "1\n").unwrap();
    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "running",
            "running 1 test\n",
            "",
            None,
            running_progress("test"),
            false,
        ))
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["promoted_to_job"], true);
    // Command ran exactly once.
    assert_eq!(
        std::fs::read_to_string(&counter).unwrap().trim(),
        "1",
        "the command must start exactly once"
    );
    // Advance to terminal; still exactly one start.
    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "completed",
            "running 1 test\n\ntest result: ok. 1 passed; 0 failed\n",
            "",
            Some(0),
            completed_progress(),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(&counter).unwrap().trim(),
        "1",
        "handoff + terminal must not re-run the command"
    );
}

/// Job final success: after handoff the job completes, its structured
/// diagnostics are queryable, and the session validation summary is passed.
#[tokio::test]
async fn handoff_job_terminal_success_produces_passed_validation_summary() {
    let client_id = "vhandoff-success";
    let runtime = runtime_with_agent_project(client_id)
        .with_validation_sync_wait(std::time::Duration::from_millis(50));
    let caps = ShellClientCapabilities {
        async_shell_jobs: true,
        structured_validation_argv: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);
    let auth = auth_context(None, true);
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let session_id = session.session_id.clone();

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let session_id = session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::CargoTest {
                        project,
                        session_id: Some(session_id),
                        cwd: None,
                        filter: Some("sum".to_string()),
                        all_targets: None,
                        all_features: None,
                        no_default_features: None,
                        features: None,
                        package: None,
                        no_run: None,
                        timeout_secs: Some(1800),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let (request, job_id) = poll_start_validation_job(&runtime, client_id).await;
    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "running",
            "running 3 tests\n",
            "",
            None,
            running_progress("test"),
            false,
        ))
        .await
        .unwrap();

    let handoff = task.await.unwrap();
    assert!(handoff.success);
    assert_eq!(handoff.output["promoted_to_job"], true);

    // Advance the job to terminal success.
    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "completed",
            "running 3 tests\n\ntest result: ok. 3 passed; 0 failed; 0 ignored\n",
            "",
            Some(0),
            completed_progress(),
            true,
        ))
        .await
        .unwrap();

    // Job status terminal + structured counts.
    let status = runtime
        .job_status_for_auth(job_id.clone(), false, None)
        .await;
    assert!(status.success);
    assert_eq!(status.output["status"], "completed");
    assert_eq!(status.output["terminal"], true);
    assert_eq!(status.output["validation"]["tool"], "cargo_test");
    assert_eq!(status.output["validation"]["kind"], "test");
    assert_eq!(status.output["validation"]["passed"], true);
    assert_eq!(status.output["validation"]["tests_detected"], true);
    assert_eq!(status.output["validation"]["tests_run_count"], 3);
    assert_eq!(status.output["validation"]["tests_passed"], 3);
    assert_eq!(status.output["validation"]["tests_failed"], 0);
    let log = runtime
        .job_log_for_auth(job_id.clone(), None, Some(200), None, None, None)
        .await;
    assert!(log.success);
    assert_eq!(log.output["validation"], status.output["validation"]);

    // The session validation summary reflects the real terminal result. The
    // handoff itself is non-terminal (accepted as a job), so the summary must
    // be computed with the job-terminal synthesis path.
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(50))
        .unwrap();
    let validation = runtime
        .validation_summary_for_session_with_jobs(&summary, 50, Some(&auth))
        .await;
    assert_eq!(validation["status"], "passed");
    let latest = &validation["latest"];
    assert_eq!(latest["tool_name"], "cargo_test");
    assert_eq!(latest["success"], true);
    assert_eq!(latest["exit_code"], 0);
    assert_eq!(
        latest["tests_run_count"],
        status.output["validation"]["tests_run_count"]
    );
}

#[tokio::test]
async fn partial_agent_job_logs_null_complete_validation_counts_everywhere() {
    let client_id = "vhandoff-partial-counts";
    let runtime = runtime_with_agent_project(client_id)
        .with_validation_sync_wait(std::time::Duration::from_millis(50));
    let caps = ShellClientCapabilities {
        async_shell_jobs: true,
        structured_validation_argv: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);
    let auth = auth_context(None, true);
    let session = runtime.sessions.start_session(Some(project.clone()), None);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::CargoTest {
                        project,
                        session_id: Some(session_id),
                        cwd: None,
                        filter: None,
                        all_targets: None,
                        all_features: None,
                        no_default_features: None,
                        features: None,
                        package: None,
                        no_run: None,
                        timeout_secs: Some(1800),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let (request, job_id) = poll_start_validation_job(&runtime, client_id).await;
    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "running",
            "running 3 tests\n",
            "",
            None,
            running_progress("test"),
            false,
        ))
        .await
        .unwrap();
    let handoff = task.await.unwrap();
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(handoff.output["promoted_to_job"], true);

    let mut stdout = (0..600)
        .map(|index| format!("progress line {index}\n"))
        .collect::<String>();
    stdout.push_str("test result: ok. 3 passed; 0 failed; 0 ignored\n");
    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "completed",
            &stdout,
            "",
            Some(0),
            completed_progress(),
            true,
        ))
        .await
        .unwrap();

    let status = runtime
        .job_status_for_auth(job_id.clone(), false, Some(&auth))
        .await;
    assert!(status.success, "{:?}", status.error);
    assert_eq!(status.output["validation"]["passed"], true);
    assert_eq!(status.output["validation"]["truncated"], true);
    for field in [
        "tests_run_count",
        "tests_passed",
        "tests_failed",
        "zero_tests_run",
    ] {
        assert!(status.output["validation"][field].is_null(), "{field}");
    }

    let log = runtime
        .job_log_for_auth(job_id.clone(), None, Some(200), Some(&auth), None, None)
        .await;
    assert!(log.success, "{:?}", log.error);
    assert_eq!(log.output["validation"]["passed"], true);
    assert_eq!(log.output["validation"]["truncated"], true);
    for field in [
        "tests_run_count",
        "tests_passed",
        "tests_failed",
        "zero_tests_run",
    ] {
        assert!(log.output["validation"][field].is_null(), "{field}");
    }

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(50))
        .unwrap();
    let validation = runtime
        .validation_summary_for_session_with_jobs(&summary, 50, Some(&auth))
        .await;
    assert_eq!(validation["status"], "passed");
    let latest = &validation["latest"];
    assert_eq!(latest["tool_name"], "cargo_test");
    for field in [
        "tests_run_count",
        "tests_passed",
        "tests_failed",
        "zero_tests_run",
    ] {
        assert!(latest[field].is_null(), "{field}");
    }
}

/// Job final failure: a non-zero job is classified as a validation failure,
/// never as an infrastructure timeout.
#[tokio::test]
async fn handoff_job_terminal_failure_is_validation_failed_not_timeout() {
    let client_id = "vhandoff-fail";
    let runtime = runtime_with_agent_project(client_id)
        .with_validation_sync_wait(std::time::Duration::from_millis(50));
    let caps = ShellClientCapabilities {
        async_shell_jobs: true,
        structured_validation_argv: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .cargo_test(
                    project,
                    None,
                    Some("failing".to_string()),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(1800),
                )
                .await
        }
    });
    let (request, job_id) = poll_start_validation_job(&runtime, client_id).await;
    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "running",
            "",
            "",
            None,
            running_progress("test"),
            false,
        ))
        .await
        .unwrap();
    let handoff = task.await.unwrap();
    assert!(handoff.success);
    assert_eq!(handoff.output["promoted_to_job"], true);

    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "failed",
            "test result: FAILED. 0 passed; 1 failed\n",
            "",
            Some(101),
            ShellJobValidationProgress {
                completed: 0,
                current_step: None,
                failed_step: Some("test".to_string()),
            },
            true,
        ))
        .await
        .unwrap();

    let status = runtime
        .job_status_for_auth(job_id.clone(), false, None)
        .await;
    assert!(status.success);
    assert_eq!(status.output["status"], "failed");
    assert_eq!(status.output["validation"]["passed"], false);
    assert_eq!(status.output["validation"]["tests_failed"], 1);
    assert_eq!(status.output["validation"]["tests_passed"], 0);
}

/// A real total-runtime timeout (job reaches `timeout`) must be classified as
/// a timeout, and the process group is reaped by the runner.
#[tokio::test]
async fn handoff_job_total_timeout_is_classified_timeout() {
    let client_id = "vhandoff-timeout";
    let runtime = runtime_with_agent_project(client_id)
        .with_validation_sync_wait(std::time::Duration::from_millis(50));
    let caps = ShellClientCapabilities {
        async_shell_jobs: true,
        structured_validation_argv: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .cargo_check(project, None, None, None, None, None, None, Some(3600))
                .await
        }
    });
    let (request, job_id) = poll_start_validation_job(&runtime, client_id).await;
    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "running",
            "",
            "",
            None,
            running_progress("check"),
            false,
        ))
        .await
        .unwrap();
    let handoff = task.await.unwrap();
    assert!(handoff.success);
    assert_eq!(handoff.output["promoted_to_job"], true);

    // The runner enforces the total budget and reports a timeout terminal.
    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "timeout",
            "Compiling webcodex...\n",
            "Command timed out after 3600 seconds\n",
            Some(-1),
            completed_progress(),
            true,
        ))
        .await
        .unwrap();

    let status = runtime
        .job_status_for_auth(job_id.clone(), false, None)
        .await;
    assert!(status.success);
    assert_eq!(status.output["status"], "timeout");
    assert_eq!(status.output["terminal"], true);
    assert_eq!(status.output["validation"]["state"], "timed_out");
    assert!(status.output["validation"]["passed"].is_null());
    assert!(status.output["validation"]["warnings_count"].is_null());
    assert!(status.output["validation"]["errors_count"].is_null());
}

/// Explicit short timeout: a requested budget at or below the sync window
/// runs synchronously to a real terminal timeout and never creates a Job.
#[tokio::test]
async fn explicit_short_timeout_never_creates_a_job() {
    let client_id = "vhandoff-short";
    let runtime = runtime_with_agent_project(client_id);
    let caps = ShellClientCapabilities {
        async_shell_jobs: true,
        structured_validation_argv: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .cargo_check(project, None, None, None, None, None, None, Some(1))
                .await
        }
    });
    // The short path enqueues a plain `run_shell`-style request (the existing
    // sync capture), not a structured validation Job.
    let request = next_patch_agent_request(&runtime, client_id)
        .await
        .expect("short validation should enqueue a shell request");
    assert_ne!(request.kind, "start_validation_job");
    complete_sync_shell_lifecycle(
        &runtime,
        client_id,
        request.request_id,
        ShellCommandExecutionState::TimedOut,
        Some(-1),
        "partial output\n",
        "Command timed out after 1 seconds",
        None,
    )
    .await;

    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["failure_kind"], "timeout");
    assert_eq!(result.output["execution_state"], "timed_out");
    assert_eq!(result.output["command_started"], true);
    assert_eq!(result.output["command_completed"], false);
    assert_cargo_result_matches_schema("cargo_check", &result);
    // No job was created.
    assert!(runtime.shell_clients.list_jobs(Some(10)).await.is_empty());
}

#[tokio::test]
async fn legacy_sync_cargo_runner_pre_spawn_failure_stays_not_started() {
    let client_id = "vhandoff-legacy-pre-spawn";
    let runtime = runtime_with_agent_project(client_id);
    let caps = ShellClientCapabilities {
        shell: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .cargo_check(project, None, None, None, None, None, None, Some(60))
                .await
        }
    });
    let request = wait_for_agent_request(&runtime, client_id).await;
    complete_sync_shell_lifecycle(
        &runtime,
        client_id,
        request.request_id,
        ShellCommandExecutionState::NotStarted,
        None,
        "",
        "",
        Some("spawn_failed: cargo executable was not available"),
    )
    .await;

    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "not_started");
    assert_eq!(result.output["command_started"], false);
    assert_eq!(result.output["command_completed"], false);
    assert_eq!(result.output["failure_kind"], "executor_unavailable");
    assert_ne!(result.output["failure_kind"], "timeout");
    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("Rejected before starting command")));
    assert_cargo_result_matches_schema("cargo_check", &result);
}

#[tokio::test]
async fn legacy_sync_cargo_runner_post_spawn_uncertainty_stays_outcome_unknown() {
    let client_id = "vhandoff-legacy-outcome-unknown";
    let runtime = runtime_with_agent_project(client_id);
    let caps = ShellClientCapabilities {
        shell: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .cargo_check(project, None, None, None, None, None, None, Some(60))
                .await
        }
    });
    let request = wait_for_agent_request(&runtime, client_id).await;
    complete_sync_shell_lifecycle(
        &runtime,
        client_id,
        request.request_id,
        ShellCommandExecutionState::OutcomeUnknown,
        None,
        "",
        "",
        Some("the Runner lost the process result after spawn"),
    )
    .await;

    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "outcome_unknown");
    assert_eq!(result.output["command_started"], true);
    assert_eq!(result.output["command_completed"], false);
    assert_eq!(result.output["failure_kind"], "outcome_unknown");
    assert_eq!(result.output["terminal"], false);
    assert_eq!(result.output["promoted_to_job"], false);
    assert!(result.output.get("job_id").is_none());
    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("Do not automatically retry")));
    assert_cargo_result_matches_schema("cargo_check", &result);
}

/// A queued hidden validation can be cancelled atomically before the Runner
/// receives its start request. The command never starts and no hidden/public
/// record survives.
/// Invalid structured Cargo arguments (option-like `--features`/`-p` values,
/// control characters, over-long values) must be rejected before any command
/// or Agent request is started — on both the sync path and the long-Job
/// handoff path.
#[tokio::test]
async fn invalid_cargo_args_fail_before_command_or_agent_request() {
    let client_id = "vhandoff-invalid-args";
    let runtime = runtime_with_agent_project(client_id)
        .with_validation_sync_wait(std::time::Duration::from_millis(50));
    let caps = ShellClientCapabilities {
        async_shell_jobs: true,
        structured_validation_argv: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);
    let auth = auth_context(None, true);

    for (label, call) in [
        (
            "features=--no-run",
            ToolCall::CargoCheck {
                project: project.clone(),
                session_id: None,
                cwd: None,
                all_targets: Some(true),
                all_features: None,
                no_default_features: None,
                features: Some("--no-run".to_string()),
                package: None,
                timeout_secs: Some(1800),
            },
        ),
        (
            "package=--all-features",
            ToolCall::CargoCheck {
                project: project.clone(),
                session_id: None,
                cwd: None,
                all_targets: Some(true),
                all_features: None,
                no_default_features: None,
                features: None,
                package: Some("--all-features".to_string()),
                timeout_secs: Some(1800),
            },
        ),
        (
            "features contains control char",
            ToolCall::CargoTest {
                project: project.clone(),
                session_id: None,
                cwd: None,
                filter: None,
                all_targets: None,
                all_features: None,
                no_default_features: None,
                features: Some("line\nbreak".to_string()),
                package: None,
                no_run: None,
                timeout_secs: Some(1800),
            },
        ),
        (
            "over-long feature value",
            ToolCall::CargoCheck {
                project: project.clone(),
                session_id: None,
                cwd: None,
                all_targets: Some(true),
                all_features: None,
                no_default_features: None,
                features: Some("a".repeat(crate::shell_protocol::CARGO_VALUE_MAX_BYTES + 1)),
                package: None,
                timeout_secs: Some(1800),
            },
        ),
    ] {
        let result = runtime.dispatch_with_auth(call, Some(&auth)).await;
        assert!(!result.success, "{label} must fail");
        assert_eq!(
            result.output["command_started"],
            serde_json::json!(false),
            "{label}: command must not start"
        );
        // No agent request may have been enqueued for the rejected call.
        assert!(
            next_patch_agent_request(&runtime, client_id)
                .await
                .is_none(),
            "{label}: no agent request may be enqueued"
        );
        assert!(
            runtime.shell_clients.list_jobs(Some(10)).await.is_empty(),
            "{label}: no job may be created"
        );
    }
}

#[tokio::test]
async fn cancel_queued_before_handoff_removes_start_request_and_hidden_record() {
    let client_id = "vhandoff-cancel-queued";
    let runtime = runtime_with_agent_project(client_id)
        .with_validation_sync_wait(std::time::Duration::from_secs(60));
    let caps = ShellClientCapabilities {
        async_shell_jobs: true,
        structured_validation_argv: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .cargo_test(
                    project,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(1800),
                )
                .await
        }
    });
    let job_id = loop {
        if let Some(job_id) = runtime
            .shell_clients
            .hidden_job_ids_for_test()
            .await
            .into_iter()
            .next()
        {
            break job_id;
        }
        tokio::task::yield_now().await;
    };
    task.abort();
    let _ = task.await;
    for _ in 0..100 {
        if runtime
            .shell_clients
            .hidden_job_ids_for_test()
            .await
            .is_empty()
        {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(runtime
        .shell_clients
        .get_hidden_job_for_auth(None, &job_id)
        .await
        .is_err());
    assert!(next_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
    assert!(runtime.shell_clients.list_jobs(Some(10)).await.is_empty());
}

/// A running hidden validation is not deleted when cancellation merely requests
/// stop. It remains internally reconcilable until the Runner confirms a
/// terminal state, while every public Job surface continues to hide it.
#[tokio::test]
async fn cancel_running_before_handoff_retains_record_until_runner_stops() {
    let client_id = "vhandoff-cancel-running";
    let runtime = runtime_with_agent_project(client_id)
        .with_validation_sync_wait(std::time::Duration::from_secs(60));
    let caps = ShellClientCapabilities {
        async_shell_jobs: true,
        structured_validation_argv: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .cargo_test(
                    project,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(1800),
                )
                .await
        }
    });
    let (request, job_id) = poll_start_validation_job(&runtime, client_id).await;
    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "running",
            "running 1 test\n",
            "",
            None,
            running_progress("test"),
            false,
        ))
        .await
        .unwrap();

    task.abort();
    let _ = task.await;
    let intent_registered = runtime
        .shell_clients
        .has_hidden_cleanup_intent_for_test(&job_id);
    let immediately_processed = runtime
        .shell_clients
        .get_hidden_job_for_auth(None, &job_id)
        .await
        .is_ok_and(|job| job.status == "stop_requested");
    assert!(
        intent_registered || immediately_processed,
        "Drop must synchronously register cleanup intent before relying on async processing"
    );
    if intent_registered {
        crate::shell_client::recovery_timeout_sweep(&runtime.shell_clients).await;
    }
    let stop = next_patch_agent_request(&runtime, client_id)
        .await
        .expect("cancellation should enqueue stop_job");
    assert_eq!(stop.kind, "stop_job");
    assert_eq!(stop.job_id.as_deref(), Some(job_id.as_str()));
    let hidden = runtime
        .shell_clients
        .get_hidden_job_for_auth(None, &job_id)
        .await
        .expect("cleanup-pending job must remain internally queryable");
    assert_eq!(hidden.status, "stop_requested");
    assert!(
        !runtime
            .job_status_for_auth(job_id.clone(), false, None)
            .await
            .success
    );
    assert!(runtime.shell_clients.list_jobs(Some(10)).await.is_empty());

    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "stopped",
            "running 1 test\n",
            "",
            None,
            completed_progress(),
            true,
        ))
        .await
        .expect("late terminal update must be accepted");
    assert!(runtime
        .shell_clients
        .get_hidden_job_for_auth(None, &job_id)
        .await
        .is_err());
}

/// handoff after client disconnect: once `job_id` is returned, the Job is
/// independent and `stop_job(confirm=true)` can stop it.
#[tokio::test]
async fn stop_job_stops_a_handoff_job() {
    let client_id = "vhandoff-stop";
    let runtime = runtime_with_agent_project(client_id)
        .with_validation_sync_wait(std::time::Duration::from_millis(50));
    let caps = ShellClientCapabilities {
        async_shell_jobs: true,
        structured_validation_argv: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);
    let auth = auth_context(None, true);
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let session_id = session.session_id.clone();

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let project = project.clone();
        let session_id = session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::CargoTest {
                        project,
                        session_id: Some(session_id),
                        cwd: None,
                        filter: None,
                        all_targets: None,
                        all_features: None,
                        no_default_features: None,
                        features: None,
                        package: None,
                        no_run: None,
                        timeout_secs: Some(1800),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let (request, job_id) = poll_start_validation_job(&runtime, client_id).await;
    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "running",
            "",
            "",
            None,
            running_progress("test"),
            false,
        ))
        .await
        .unwrap();
    let handoff = task.await.unwrap();
    assert!(handoff.success);
    assert_eq!(handoff.output["promoted_to_job"], true);

    let stopped = runtime
        .dispatch_with_auth(
            ToolCall::StopJob {
                project: project.clone(),
                job_id: job_id.clone(),
                session_id: Some(session.session_id.clone()),
                confirm: true,
            },
            Some(&auth),
        )
        .await;
    assert!(stopped.success, "{:?}", stopped.error);
    assert_eq!(stopped.output["status_after"], "stop_requested");
}

/// Local / Agent parity: both executors return the same field semantics for
/// terminal validation results (`execution_source`, `execution_state`,
/// `passed`, bounded tails, `effective_timeout_secs`).
#[tokio::test]
async fn terminal_validation_result_fields_are_consistent_between_executors() {
    let client_id = "vhandoff-local";
    // Local project: the runtime's own project map is used (no agent).
    let tmp = tempfile::tempdir().unwrap();
    let _ = tmp;
    let runtime = runtime_with_agent_project(client_id)
        .with_validation_sync_wait(std::time::Duration::from_millis(50));
    let caps = ShellClientCapabilities {
        async_shell_jobs: true,
        structured_validation_argv: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .cargo_check(project, None, None, None, None, None, None, Some(120))
                .await
        }
    });
    // Short path (120 <= 90? No: 120 > 90, so it uses the agent Job path).
    // This exercises the agent terminal result path and returns a full
    // terminal projection in-window.
    let (request, job_id) = poll_start_validation_job(&runtime, client_id).await;
    runtime
        .shell_clients
        .update_job(cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "completed",
            "Finished `dev` profile [unoptimized + debuginfo] target(s)\n",
            "",
            Some(0),
            completed_progress(),
            true,
        ))
        .await
        .unwrap();
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    for field in [
        "execution_source",
        "execution_state",
        "passed",
        "exit_code",
        "stdout_tail",
        "stderr_tail",
        "effective_timeout_secs",
    ] {
        assert!(
            result.output.get(field).is_some(),
            "terminal result missing {field}"
        );
    }
    assert_eq!(result.output["execution_state"], "completed");
    assert_eq!(result.output["passed"], true);
}

#[tokio::test]
async fn legacy_agent_default_check_and_test_use_one_synchronous_120_second_execution() {
    for (index, tool_name) in ["cargo_check", "cargo_test"].into_iter().enumerate() {
        let client_id = format!("vhandoff-legacy-default-{index}");
        let runtime = runtime_with_agent_project(&client_id);
        register_agent(
            &runtime,
            &client_id,
            None,
            ShellClientCapabilities::default(),
        )
        .await;
        let project = agent_test_project_id(&client_id);
        let auth = auth_context(None, true);
        let mut task = tokio::spawn({
            let runtime = runtime.clone();
            let auth = auth.clone();
            async move {
                if tool_name == "cargo_check" {
                    runtime
                        .dispatch_with_auth(
                            ToolCall::CargoCheck {
                                project,
                                session_id: None,
                                cwd: None,
                                all_targets: None,
                                all_features: None,
                                no_default_features: None,
                                features: None,
                                package: None,
                                timeout_secs: None,
                            },
                            Some(&auth),
                        )
                        .await
                } else {
                    runtime
                        .dispatch_with_auth(
                            ToolCall::CargoTest {
                                project,
                                session_id: None,
                                cwd: None,
                                filter: None,
                                all_targets: None,
                                all_features: None,
                                no_default_features: None,
                                features: None,
                                package: None,
                                no_run: None,
                                timeout_secs: None,
                            },
                            Some(&auth),
                        )
                        .await
                }
            }
        });
        let request = tokio::select! {
            result = &mut task => panic!("legacy validation completed before enqueue: {:?}", result.unwrap()),
            request = wait_for_agent_request(&runtime, &client_id) => request,
        };
        assert_ne!(request.kind, "start_validation_job");
        runtime
            .shell_clients
            .complete(crate::shell_protocol::ShellAgentResultRequest {
                client_id: client_id.clone(),
                agent_instance_id: "inst".to_string(),
                request_id: request.request_id,
                exit_code: Some(0),
                stdout: Some(if tool_name == "cargo_test" {
                    "running 1 test\n\ntest result: ok. 1 passed; 0 failed; 0 ignored\n".to_string()
                } else {
                    "Finished `dev` profile\n".to_string()
                }),
                stderr: Some(String::new()),
                duration_ms: Some(5),
                error: None,
            })
            .await
            .unwrap();
        let result = task.await.unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["execution_state"], "completed");
        assert_eq!(result.output["command_started"], true);
        assert_eq!(result.output["command_completed"], true);
        assert_eq!(result.output["effective_timeout_secs"], 120);
        assert_eq!(result.output["promoted_to_job"], false);
        assert_eq!(result.output["async_handoff_available"], false);
        assert!(runtime.shell_clients.list_jobs(Some(10)).await.is_empty());
        assert_cargo_result_matches_schema(tool_name, &result);
    }
}

#[tokio::test]
async fn legacy_agent_explicit_120_runs_but_121_rejects_before_start() {
    let client_id = "vhandoff-legacy-boundary";
    let runtime = runtime_with_agent_project(client_id);
    register_agent(
        &runtime,
        client_id,
        None,
        ShellClientCapabilities::default(),
    )
    .await;
    let project = agent_test_project_id(client_id);
    let auth = auth_context(None, true);

    let mut accepted = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::CargoCheck {
                        project,
                        session_id: None,
                        cwd: None,
                        all_targets: None,
                        all_features: None,
                        no_default_features: None,
                        features: None,
                        package: None,
                        timeout_secs: Some(120),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = tokio::select! {
        result = &mut accepted => panic!("legacy validation completed before enqueue: {:?}", result.unwrap()),
        request = wait_for_agent_request(&runtime, client_id) => request,
    };
    assert_ne!(request.kind, "start_validation_job");
    runtime
        .shell_clients
        .complete(crate::shell_protocol::ShellAgentResultRequest {
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some("Finished `dev` profile\n".to_string()),
            stderr: Some(String::new()),
            duration_ms: Some(5),
            error: None,
        })
        .await
        .unwrap();
    let accepted = accepted.await.unwrap();
    assert!(accepted.success);
    assert_eq!(accepted.output["effective_timeout_secs"], 120);

    let rejected = runtime
        .dispatch_with_auth(
            ToolCall::CargoCheck {
                project,
                session_id: None,
                cwd: None,
                all_targets: None,
                all_features: None,
                no_default_features: None,
                features: None,
                package: None,
                timeout_secs: Some(121),
            },
            Some(&auth),
        )
        .await;
    assert!(!rejected.success);
    assert_eq!(rejected.output["failure_kind"], "capability_unavailable");
    assert_eq!(rejected.output["command_started"], false);
    assert_eq!(rejected.output["command_completed"], false);
    assert!(next_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
    assert_cargo_result_matches_schema("cargo_check", &rejected);
}

#[test]
fn local_inspect_validation_stays_on_synchronous_sandbox_path() {
    assert!(!super::super::cargo::local_validation_should_handoff(
        180,
        90,
        Some(crate::command_sandbox::INSPECT_SANDBOX_MODE),
    ));
    assert!(super::super::cargo::local_validation_should_handoff(
        180, 90, None,
    ));
}

#[tokio::test]
async fn local_fast_fmt_check_returns_terminal_without_public_job() {
    let tmp = tempfile::tempdir().unwrap();
    write_local_validation_crate(tmp.path(), "pub fn value() -> u32 {\n    1\n}\n");
    let runtime = runtime_with_project(tmp.path(), "demo")
        .with_validation_sync_wait(std::time::Duration::from_secs(5));
    let config = local_project_config(&tmp.path().to_string_lossy());
    let result = runtime
        .run_readonly_validation_local_job(
            "cargo_fmt",
            "demo",
            &config,
            None,
            "cargo fmt -- --check",
            validation_adapter_for_tool("cargo_fmt").unwrap(),
            ValidationCommandOptions {
                check: true,
                ..Default::default()
            },
            ExecutionPurpose::Format,
            10,
            5,
        )
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["executor"], "local");
    assert_eq!(result.output["shell"], "direct_argv");
    assert_eq!(result.output["promoted_to_job"], false);
    assert_eq!(result.output["terminal"], true);
    assert!(result.output.get("job_id").is_none());
    assert!(runtime.local_jobs.lock().await.is_empty());
    assert_cargo_result_matches_schema("cargo_fmt", &result);
}

#[tokio::test]
async fn local_long_test_hides_then_hands_off_once_with_structured_terminal() {
    let tmp = tempfile::tempdir().unwrap();
    write_local_validation_crate(
        tmp.path(),
        r#"
#[cfg(test)]
mod tests {
    #[test]
    fn runs_once() {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("starts.txt")
            .unwrap();
        writeln!(file, "start").unwrap();
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}
"#,
    );
    let runtime = runtime_with_project(tmp.path(), "demo")
        .with_validation_sync_wait(std::time::Duration::from_millis(500));
    let config = local_project_config(&tmp.path().to_string_lossy());
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let config = config.clone();
        async move {
            runtime
                .run_readonly_validation_local_job(
                    "cargo_test",
                    "demo",
                    &config,
                    None,
                    "cargo test",
                    validation_adapter_for_tool("cargo_test").unwrap(),
                    ValidationCommandOptions::default(),
                    ExecutionPurpose::Test,
                    10,
                    1,
                )
                .await
        }
    });

    let hidden_job_id = loop {
        let hidden = {
            let jobs = runtime.local_jobs.lock().await;
            jobs.iter()
                .find(|(_, record)| !record.is_public())
                .map(|(job_id, _)| job_id.clone())
        };
        if let Some(job_id) = hidden {
            break job_id;
        }
        tokio::task::yield_now().await;
    };
    assert!(
        runtime.list_jobs_for_auth(None, None, None).await.output["jobs"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let handoff = task.await.unwrap();
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(handoff.output["promoted_to_job"], true);
    assert_eq!(handoff.output["job_id"], hidden_job_id);
    assert_eq!(handoff.output["executor"], "local");
    assert_cargo_result_matches_schema("cargo_test", &handoff);
    let observation_token = handoff.output["observation_token"]
        .as_str()
        .expect("local validation handoff observation token")
        .to_string();
    let observed = runtime
        .observe_jobs_for_auth(
            vec![ObserveJobsItem {
                job_id: hidden_job_id.clone(),
                after_observation_token: Some(observation_token.clone()),
            }],
            40,
            None,
            None,
        )
        .await;
    assert!(observed.success, "{:?}", observed.error);
    assert_eq!(observed.output["items"][0]["success"], true);
    assert_eq!(
        observed.output["items"][0]["output"]["observation_token"],
        observation_token
    );

    let status = wait_for_local_job_terminal(&runtime, &hidden_job_id).await;
    assert_eq!(status.output["status"], "completed");
    assert_eq!(status.output["validation"]["tool"], "cargo_test");
    assert_eq!(status.output["validation"]["passed"], true);
    assert_eq!(status.output["validation"]["tests_run_count"], 1);
    assert_eq!(status.output["validation"]["tests_passed"], 1);
    let log = runtime
        .job_log(hidden_job_id.clone(), None, Some(200))
        .await;
    assert!(log.success);
    assert_eq!(log.output["validation"], status.output["validation"]);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("starts.txt"))
            .unwrap()
            .lines()
            .count(),
        1,
        "the local validation command must execute exactly once"
    );
}

#[tokio::test]
async fn local_job_status_uses_bounded_logs_and_nulls_complete_counts() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_project(tmp.path(), "demo");
    let job_id = "bounded-local-validation";
    let dir = tmp.path().join(".codex/jobs").join(job_id);
    std::fs::create_dir_all(&dir).unwrap();
    let (record, _) = crate::tool_runtime::local_jobs::LocalJobRecord::initialize(
        "demo".to_string(),
        dir.clone(),
    )
    .unwrap();
    std::fs::write(
        dir.join("metadata.json"),
        serde_json::to_string(&json!({
            "job_id": job_id,
            "project": "demo",
            "command": "cargo test",
            "kind": "validation",
            "created_at": 1,
            "started_at": 1,
            "validation_tool": "cargo_test",
            "validation_kind": "test",
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("status"), "completed").unwrap();
    std::fs::write(dir.join("exit_code"), "0").unwrap();
    std::fs::write(dir.join("finished_at"), "2").unwrap();
    let mut stdout = (0..(crate::tool_runtime::helpers::MAX_LOCAL_LOG_LINES + 100))
        .map(|index| format!("progress line {index}\n"))
        .collect::<String>();
    stdout.push_str("test result: ok. 7 passed; 0 failed; 0 ignored\n");
    std::fs::write(dir.join("stdout.log"), stdout).unwrap();
    std::fs::write(dir.join("stderr.log"), "").unwrap();
    record.observe().unwrap();
    record.mark_terminal();
    runtime
        .local_jobs
        .lock()
        .await
        .insert(job_id.to_string(), record.clone());

    let bounded = record.read_log_lines(
        "stdout.log",
        None,
        Some(crate::tool_runtime::helpers::MAX_LOCAL_LOG_LINES),
    );
    assert!(bounded.3);
    assert!(bounded.0.lines().count() <= crate::tool_runtime::helpers::MAX_LOCAL_LOG_LINES);

    let status = runtime.job_status(job_id.to_string()).await;
    assert!(status.success, "{:?}", status.error);
    assert_eq!(status.output["validation"]["passed"], true);
    assert_eq!(status.output["validation"]["truncated"], true);
    for field in [
        "tests_run_count",
        "tests_passed",
        "tests_failed",
        "zero_tests_run",
    ] {
        assert!(status.output["validation"][field].is_null(), "{field}");
    }
}

#[tokio::test]
async fn local_validation_stop_job_terminates_descendant_process_group() {
    let tmp = tempfile::tempdir().unwrap();
    write_local_validation_crate(
        tmp.path(),
        r#"
#[cfg(test)]
mod tests {
    #[test]
    fn descendants_are_reaped() {
        let child = std::process::Command::new("sh")
            .arg("-c")
            .arg("sleep 30")
            .spawn()
            .unwrap();
        std::fs::write("child.pid", child.id().to_string()).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}
"#,
    );
    let runtime = runtime_with_project(tmp.path(), "demo")
        .with_validation_sync_wait(std::time::Duration::from_millis(50));
    let config = local_project_config(&tmp.path().to_string_lossy());
    let handoff = runtime
        .run_readonly_validation_local_job(
            "cargo_test",
            "demo",
            &config,
            None,
            "cargo test",
            validation_adapter_for_tool("cargo_test").unwrap(),
            ValidationCommandOptions::default(),
            ExecutionPurpose::Test,
            60,
            1,
        )
        .await;
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(handoff.output["promoted_to_job"], true);
    let job_id = handoff.output["job_id"].as_str().unwrap().to_string();

    let child_pid = loop {
        if let Ok(value) = std::fs::read_to_string(tmp.path().join("child.pid")) {
            break value.trim().parse::<u32>().unwrap();
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    };
    let stopped = runtime.stop_job(job_id.clone()).await;
    assert!(stopped.success, "{:?}", stopped.error);
    let status = wait_for_local_job_terminal(&runtime, &job_id).await;
    assert_eq!(status.output["status"], "stopped");
    let descendant_is_running = || {
        std::fs::read_to_string(format!("/proc/{child_pid}/stat"))
            .ok()
            .and_then(|stat| stat.split_whitespace().nth(2).map(str::to_string))
            .is_some_and(|state| state != "Z" && state != "X")
    };
    for _ in 0..200 {
        if !descendant_is_running() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        !descendant_is_running(),
        "descendant process remained executable after stop_job"
    );
}

#[tokio::test]
async fn local_validation_total_timeout_is_terminal_and_structured() {
    let tmp = tempfile::tempdir().unwrap();
    write_local_validation_crate(
        tmp.path(),
        r#"
#[cfg(test)]
mod tests {
    #[test]
    fn times_out() {
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}
"#,
    );
    let runtime = runtime_with_project(tmp.path(), "demo")
        .with_validation_sync_wait(std::time::Duration::from_millis(50));
    let config = local_project_config(&tmp.path().to_string_lossy());
    let handoff = runtime
        .run_readonly_validation_local_job(
            "cargo_test",
            "demo",
            &config,
            None,
            "cargo test",
            validation_adapter_for_tool("cargo_test").unwrap(),
            ValidationCommandOptions::default(),
            ExecutionPurpose::Test,
            1,
            1,
        )
        .await;
    assert!(handoff.success, "{:?}", handoff.error);
    let job_id = handoff.output["job_id"].as_str().unwrap().to_string();
    let status = wait_for_local_job_terminal(&runtime, &job_id).await;
    assert_eq!(status.output["status"], "timeout");
    assert_eq!(status.output["validation"]["state"], "timed_out");
    assert!(status.output["validation"]["passed"].is_null());
    assert!(status.output["validation"]["tests_run_count"].is_null());
}

/// `cargo_fmt(check=false)` never auto-promotes: it keeps the existing
/// synchronous execution semantics and must not modify source after the tool
/// returns.
#[tokio::test]
async fn cargo_fmt_mutating_never_auto_promotes() {
    let client_id = "vhandoff-fmt-mutate";
    let runtime = runtime_with_agent_project(client_id);
    let caps = ShellClientCapabilities {
        async_shell_jobs: true,
        structured_validation_argv: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .cargo_fmt(project, None, Some(false), Some(120))
                .await
        }
    });
    let request = next_patch_agent_request(&runtime, client_id)
        .await
        .expect("cargo fmt (mutating) should enqueue a shell request");
    assert_ne!(request.kind, "start_validation_job");
    runtime
        .shell_clients
        .complete(crate::shell_protocol::ShellAgentResultRequest {
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some("".to_string()),
            stderr: Some(String::new()),
            duration_ms: Some(5),
            error: None,
        })
        .await
        .unwrap();
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_ne!(result.output["promoted_to_job"], true);
    assert_eq!(result.output["command_completed"], true);
    // No job was created.
    assert!(runtime.shell_clients.list_jobs(Some(10)).await.is_empty());
}

#[tokio::test]
async fn cargo_fmt_mutating_post_spawn_uncertainty_forbids_blind_retry() {
    let client_id = "vhandoff-fmt-mutate-unknown";
    let runtime = runtime_with_agent_project(client_id);
    let caps = ShellClientCapabilities {
        shell: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .cargo_fmt(project, None, Some(false), Some(60))
                .await
        }
    });
    let request = wait_for_agent_request(&runtime, client_id).await;
    complete_sync_shell_lifecycle(
        &runtime,
        client_id,
        request.request_id,
        ShellCommandExecutionState::OutcomeUnknown,
        None,
        "",
        "",
        Some("formatting process result was lost after spawn"),
    )
    .await;

    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "outcome_unknown");
    assert_eq!(result.output["failure_kind"], "outcome_unknown");
    assert_eq!(result.output["command_started"], true);
    assert_eq!(result.output["command_completed"], false);
    assert_eq!(result.output["terminal"], false);
    assert_eq!(result.output["promoted_to_job"], false);
    assert!(result.output.get("job_id").is_none());
    let error = result.error.as_deref().unwrap_or_default();
    assert!(error.contains("Do not automatically retry"), "{error}");
    assert!(error.contains("inspect the actual Job, process, service, or target state"));
    assert_cargo_result_matches_schema("cargo_fmt", &result);
    assert!(runtime.shell_clients.list_jobs(Some(10)).await.is_empty());
}

/// Cargo outputs use mutually exclusive strict branches for public Job
/// handoff, terminal execution, and pre-start rejection.
#[test]
fn cargo_output_schema_enforces_handoff_terminal_and_rejection_branches() {
    use crate::tool_runtime::registry::output_schema_for_tool;
    use crate::tool_runtime::startup_brief::validate_schema_instance_for_test;

    let schema = output_schema_for_tool("cargo_test");
    let accepts =
        |value: &serde_json::Value| validate_schema_instance_for_test(value, &schema).is_ok();

    let handoff = json!({
        "success": true,
        "output": {
            "project": "agent:x:y",
            "command_summary": "cargo test",
            "cwd": ".",
            "shell": "configured",
            "executor": "agent",
            "execution_source": "cargo_test",
            "purpose": "test",
            "execution_state": "running",
            "job_id": "job-123",
            "job_status": "running",
            "promoted_to_job": true,
            "command_started": true,
            "command_completed": false,
            "effective_timeout_secs": 1800,
            "sync_wait_secs": 90,
            "terminal": false,
            "stdout_tail": "",
            "stderr_tail": "",
            "stdout_lines": 0,
            "stderr_lines": 0,
            "stdout_truncated": false,
            "stderr_truncated": false
        }
    });
    assert!(accepts(&handoff), "handoff should validate");

    for (name, mutate) in [
        ("missing job_id", 0_u8),
        ("null job_id", 1),
        ("command_completed", 2),
        ("terminal", 3),
        ("passed", 4),
        ("timeout failure", 5),
    ] {
        let mut invalid = handoff.clone();
        let output = invalid["output"].as_object_mut().unwrap();
        match mutate {
            0 => {
                output.remove("job_id");
            }
            1 => {
                output.insert("job_id".to_string(), serde_json::Value::Null);
            }
            2 => {
                output.insert("command_completed".to_string(), json!(true));
            }
            3 => {
                output.insert("terminal".to_string(), json!(true));
            }
            4 => {
                output.insert("passed".to_string(), json!(false));
            }
            5 => {
                output.insert("failure_kind".to_string(), json!("timeout"));
            }
            _ => unreachable!(),
        }
        assert!(!accepts(&invalid), "handoff misuse should fail: {name}");
    }

    let terminal = json!({
        "success": true,
        "output": {
            "project": "agent:x:y",
            "command_summary": "cargo test",
            "cwd": ".",
            "shell": "configured",
            "executor": "agent",
            "execution_source": "cargo_test",
            "purpose": "test",
            "execution_state": "completed",
            "passed": true,
            "exit_code": 0,
            "duration_ms": 5,
            "stdout_tail": "running 1 test\ntest result: ok. 1 passed; 0 failed\n",
            "stderr_tail": "",
            "stdout_lines": 2,
            "stderr_lines": 0,
            "stdout_truncated": false,
            "stderr_truncated": false,
            "command_started": true,
            "command_completed": true,
            "promoted_to_job": false,
            "effective_timeout_secs": 1800,
            "sync_wait_secs": 90,
            "terminal": true,
            "tests_detected": true,
            "tests_run_count": 1,
            "tests_passed": 1,
            "tests_failed": 0,
            "zero_tests_run": false,
            "diagnostics": {},
            "session_recorded": true,
            "session_id": "wc_sess_test",
            "session_event_id": "evt_test",
            "permission": {"policy": "trusted_agent"}
        }
    });
    assert!(accepts(&terminal), "terminal result should validate");

    let mut missing_passed = terminal.clone();
    missing_passed["output"]
        .as_object_mut()
        .unwrap()
        .remove("passed");
    assert!(!accepts(&missing_passed));
    let mut promoted_terminal = terminal.clone();
    promoted_terminal["output"]["promoted_to_job"] = json!(true);
    assert!(!accepts(&promoted_terminal));
    let mut missing_cargo_field = terminal.clone();
    missing_cargo_field["output"]
        .as_object_mut()
        .unwrap()
        .remove("tests_passed");
    assert!(!accepts(&missing_cargo_field));
    let mut unknown_field = terminal.clone();
    unknown_field["output"]["unexpected"] = json!(true);
    assert!(!accepts(&unknown_field));

    let timeout = json!({
        "success": false,
        "error": "cargo command failed",
        "output": {
            "project": "agent:x:y",
            "command_summary": "cargo test",
            "cwd": ".",
            "shell": "configured",
            "executor": "agent",
            "execution_source": "cargo_test",
            "purpose": "test",
            "execution_state": "timed_out",
            "passed": false,
            "failure_kind": "timeout",
            "exit_code": null,
            "duration_ms": 1000,
            "stdout_tail": "",
            "stderr_tail": "timed out",
            "stdout_lines": 0,
            "stderr_lines": 1,
            "stdout_truncated": false,
            "stderr_truncated": false,
            "command_started": true,
            "command_completed": false,
            "promoted_to_job": false,
            "effective_timeout_secs": 1,
            "sync_wait_secs": 1,
            "terminal": true,
            "tests_detected": false,
            "tests_run_count": null,
            "tests_passed": null,
            "tests_failed": null,
            "zero_tests_run": null,
            "diagnostics": {}
        }
    });
    assert!(accepts(&timeout), "terminal timeout should validate");

    let mut outcome_unknown = timeout.clone();
    outcome_unknown["error"] =
        json!("Command execution outcome is unknown; do not automatically retry");
    outcome_unknown["output"]["execution_state"] = json!("outcome_unknown");
    outcome_unknown["output"]["failure_kind"] = json!("outcome_unknown");
    outcome_unknown["output"]["terminal"] = json!(false);
    assert!(
        accepts(&outcome_unknown),
        "outcome_unknown should have an intentional strict branch"
    );
    let mut unknown_claiming_completion = outcome_unknown.clone();
    unknown_claiming_completion["output"]["command_completed"] = json!(true);
    assert!(
        !accepts(&unknown_claiming_completion),
        "outcome_unknown must not claim command completion"
    );
    let mut unknown_with_job = outcome_unknown.clone();
    unknown_with_job["output"]["job_id"] = json!("fabricated-job");
    assert!(
        !accepts(&unknown_with_job),
        "outcome_unknown must not fabricate a Job handle"
    );

    let rejection = json!({
        "success": false,
        "error": "Runner capability unavailable",
        "output": {
            "execution_source": "cargo_test",
            "command_started": false,
            "command_completed": false,
            "failure_kind": "capability_unavailable"
        }
    });
    assert!(accepts(&rejection), "pre-start rejection should validate");
}

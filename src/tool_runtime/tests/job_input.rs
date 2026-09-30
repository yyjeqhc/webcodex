use super::support::*;
use crate::runner_protocol::{RunnerCapabilities, RunnerResultRequest};
use crate::tool_runtime::{ToolCall, ToolRuntime};
use serde_json::json;
use webcodex_core::runner_operation::RunnerOperation;

async fn fixture(enabled: bool) -> (tempfile::TempDir, ToolRuntime, String) {
    let root = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    register_agent_with_projects(
        &runtime,
        "interactive",
        None,
        RunnerCapabilities {
            job_process_input: enabled,
            structured_process_argv: true,
            ..Default::default()
        },
        vec![registered_project("demo", &root.path().to_string_lossy())],
    )
    .await;
    (root, runtime, "agent:interactive:demo".into())
}
fn start(project: &str) -> ToolCall {
    ToolCall::from_tool_name(
        "run_process",
        json!({"project":project,"executable":"python3",
        "args":["-u","-c","input()"],"interactive":true,"timeout_secs":1}),
    )
    .unwrap()
}

#[tokio::test]
async fn interactive_start_is_public_without_waiting_and_input_keeps_exact_job() {
    let (_root, runtime, project) = fixture(true).await;
    let auth = auth_context(None, true);
    let started = runtime
        .dispatch_with_auth(start(&project), Some(&auth))
        .await;
    assert!(started.success, "{started:?}");
    assert_eq!(started.output["interactive"], true);
    let request = wait_for_patch_agent_request(&runtime, "interactive").await;
    assert_eq!(request.kind, "start_interactive_process_job");
    let job_id = request.job_id.clone().unwrap();
    let job = runtime
        .runner_registry
        .get_job_for_auth(None, &job_id)
        .await
        .unwrap();
    let metadata = job.structured_execution.unwrap();
    assert_eq!(metadata.execution_source, "run_process_interactive");
    assert!(metadata.validation_identity.is_none());
    assert!(metadata.validation_tool.is_none());
    let schema = crate::tool_runtime::registry::output_schema_for_tool("run_process");
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &json!({"success":true,"output":started.output,"error":null}),
        &schema,
    )
    .unwrap();
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let job_id = job_id.clone();
        let auth = auth.clone();
        async move {
            runtime.dispatch_with_auth(ToolCall::from_tool_name("job_write_input",json!({
            "project":project,"job_id":job_id,"input_id":"request-1","data":"hello\n","close":true
        })).unwrap(),Some(&auth)).await
        }
    });
    let input = wait_for_patch_agent_request(&runtime, "interactive").await;
    assert_eq!(input.kind, "job_write_input");
    assert!(
        input.job_id.is_none(),
        "input is an RPC, not another Job start"
    );
    let RunnerOperation::JobInput(payload) = input.decode_operation().unwrap() else {
        panic!("typed input expected")
    };
    assert_eq!(payload.job_id, job_id);
    assert_eq!(payload.runner_instance_id, "inst");
    assert_eq!(payload.project, project);
    assert_eq!(payload.data, "hello\n");
    complete_patch_agent_request(
        &runtime,
        "interactive",
        &input.request_id,
        0,
        &json!({"input_id":"request-1","state":"closed","bytes_written":6,"stdin_closed":true})
            .to_string(),
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    assert_eq!(result.output["state"], "closed");
    assert_eq!(result.output["job_id"], job_id);
    assert!(probe_patch_agent_request(&runtime, "interactive")
        .await
        .is_none());
}

#[tokio::test]
async fn interactive_unsupported_and_conflicting_options_do_not_enqueue() {
    let (_root, runtime, project) = fixture(false).await;
    let result = runtime
        .dispatch_with_auth(start(&project), Some(&auth_context(None, true)))
        .await;
    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "not_started");
    assert!(probe_patch_agent_request(&runtime, "interactive")
        .await
        .is_none());
    for extra in [
        json!({"stdin":"initial bytes"}),
        json!({"args":["bad\u{0000}arg"]}),
    ] {
        let mut args = json!({"project":project,"executable":"python3","interactive":true});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let result = runtime
            .dispatch_with_auth(
                ToolCall::from_tool_name("run_process", args).unwrap(),
                Some(&auth_context(None, true)),
            )
            .await;
        assert!(!result.success);
        assert_eq!(result.output["execution_state"], "not_started");
        assert!(probe_patch_agent_request(&runtime, "interactive")
            .await
            .is_none());
    }
}

#[tokio::test]
async fn malformed_input_receipt_remains_unknown_not_a_successful_write() {
    for receipt in [
        json!({"input_id":"different","state":"written","bytes_written":3,"stdin_closed":false}),
        json!({"input_id":"x","state":"written","bytes_written":2,"stdin_closed":false}),
        json!({"input_id":"x","state":"closed","bytes_written":3,"stdin_closed":true}),
    ] {
        let (_root, runtime, project) = fixture(true).await;
        let auth = auth_context(None, true);
        assert!(
            runtime
                .dispatch_with_auth(start(&project), Some(&auth))
                .await
                .success
        );
        let job_id = wait_for_patch_agent_request(&runtime, "interactive")
            .await
            .job_id
            .unwrap();
        let task = tokio::spawn({
            let runtime = runtime.clone();
            let auth = auth.clone();
            async move {
                runtime
                    .dispatch_with_auth(
                        ToolCall::from_tool_name(
                            "job_write_input",
                            json!({
                                "project":project,"job_id":job_id,"input_id":"x","data":"abc"
                            }),
                        )
                        .unwrap(),
                        Some(&auth),
                    )
                    .await
            }
        });
        let request = wait_for_patch_agent_request(&runtime, "interactive").await;
        runtime
            .runner_registry
            .complete(RunnerResultRequest {
                client_id: "interactive".into(),
                runner_instance_id: "inst".into(),
                request_id: request.request_id,
                exit_code: Some(0),
                stdout: Some(receipt.to_string()),
                stderr: None,
                stdout_truncated: false,
                stderr_truncated: false,
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();
        let result = task.await.unwrap();
        assert!(!result.success);
        assert_eq!(result.output["execution_state"], "outcome_unknown");
        assert!(probe_patch_agent_request(&runtime, "interactive")
            .await
            .is_none());
    }
}

#[tokio::test]
async fn revoked_capability_at_dequeue_never_exposes_queued_input_or_starts_a_process() {
    for queued_start in [true, false] {
        let (root, runtime, project) = fixture(true).await;
        assert!(
            runtime
                .dispatch_with_auth(start(&project), Some(&auth_context(None, true)))
                .await
                .success
        );
        let rx = if queued_start {
            None
        } else {
            let id = wait_for_patch_agent_request(&runtime, "interactive")
                .await
                .job_id
                .unwrap();
            Some(
                runtime
                    .runner_registry
                    .enqueue_job_input(
                        crate::runner_http::runner_access_from_auth(Some(&auth_context(
                            None, true,
                        )))
                        .as_ref(),
                        &project,
                        &id,
                        "x".into(),
                        "private-input".into(),
                        false,
                    )
                    .await
                    .unwrap()
                    .1,
            )
        };
        register_agent_with_projects(
            &runtime,
            "interactive",
            None,
            RunnerCapabilities {
                structured_process_argv: true,
                job_process_input: false,
                ..Default::default()
            },
            vec![registered_project("demo", &root.path().to_string_lossy())],
        )
        .await;
        assert!(
            probe_patch_agent_request(&runtime, "interactive")
                .await
                .is_none(),
            "stale bytes reached Runner"
        );
        if let Some(rx) = rx {
            let result = rx.await.unwrap();
            assert!(!result.success);
            assert_eq!(result.request_dispatched, Some(false));
            assert_eq!(
                result.error.as_deref(),
                Some("job_input_capability_unavailable")
            );
        }
    }
}

#[tokio::test]
async fn existing_job_input_cannot_be_retargeted_to_another_project_or_principal() {
    let (_root, runtime, project) = fixture(true).await;
    let owner = auth_context(None, true);
    assert!(
        runtime
            .dispatch_with_auth(start(&project), Some(&owner))
            .await
            .success
    );
    let id = wait_for_patch_agent_request(&runtime, "interactive")
        .await
        .job_id
        .unwrap();
    for (target, auth) in [
        ("agent:interactive:other", owner),
        (project.as_str(), auth_context(Some("foreign"), false)),
    ] {
        let result = runtime
            .dispatch_with_auth(
                ToolCall::from_tool_name(
                    "job_write_input",
                    json!({
                        "project":target,"job_id":id,"input_id":"x","data":"abc"
                    }),
                )
                .unwrap(),
                Some(&auth),
            )
            .await;
        assert!(!result.success);
        assert!(probe_patch_agent_request(&runtime, "interactive")
            .await
            .is_none());
    }
}

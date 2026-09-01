//! Whole-file write effect-boundary tests.

use super::super::*;
use super::support::*;
use crate::shell_protocol::ShellClientCapabilities;
use serde_json::json;

async fn write_runtime(client_id: &str) -> (ToolRuntime, String) {
    let runtime = runtime_with_agent_project(client_id);
    register_agent(
        &runtime,
        client_id,
        None,
        ShellClientCapabilities {
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client_id);
    (runtime, project)
}

fn assert_outcome_unknown(result: &ToolResult) {
    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "outcome_unknown");
    assert!(result.output["state_changed"].is_null());
    assert_eq!(result.output["error_kind"], "outcome_unknown");
    assert_eq!(result.output["failure_kind"], "outcome_unknown");
    assert_eq!(
        result.output["recovery_action"],
        "inspect_workspace_before_retry"
    );
    assert_eq!(result.output["recovery_kind"], "reobserve");
    let error = result.error.as_deref().expect("model-facing uncertainty");
    assert!(error.contains("outcome is unknown"), "{error}");
    assert!(error.contains("Inspect current workspace state"), "{error}");
    assert!(!error.contains("No files were modified"), "{error}");

    let schema = crate::tool_runtime::registry::output_schema_for_tool("write_project_file");
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &serde_json::to_value(result).unwrap(),
        &schema,
    )
    .unwrap_or_else(|schema_error| {
        panic!("write outcome_unknown result must match output schema: {schema_error}")
    });
}

#[tokio::test]
async fn write_project_file_requires_sha_for_overwrite_before_enqueue() {
    let (runtime, project) = write_runtime("write-guard").await;
    let result = runtime
        .write_project_file(
            project,
            "existing.txt".to_string(),
            "replacement".to_string(),
            Some(true),
            None,
        )
        .await;

    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "not_started");
    assert_eq!(result.output["state_changed"], false);
    assert_eq!(result.output["error_kind"], "missing_expected_sha256");
    assert_eq!(result.output["recovery_kind"], "fix_input");
    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("No files were modified")));
    assert!(probe_patch_agent_request(&runtime, "write-guard")
        .await
        .is_none());
}

#[tokio::test]
async fn write_project_file_dropped_waiter_after_dispatch_is_outcome_unknown() {
    let (runtime, project) = write_runtime("write-drop").await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .write_project_file(
                    project,
                    "new.txt".to_string(),
                    "hello\n".to_string(),
                    None,
                    None,
                )
                .await
        }
    });

    let request = wait_for_patch_agent_request(&runtime, "write-drop").await;
    assert_eq!(request.kind, "file_write_project_file");
    let payload: serde_json::Value =
        serde_json::from_str(request.content.as_deref().expect("write payload")).unwrap();
    assert_eq!(payload["overwrite"], false);
    assert!(payload.get("expected_content_prefix").is_none());
    assert_eq!(
        runtime
            .shell_clients
            .cancel_request_dispatch_state(&request.request_id)
            .await,
        Some(true)
    );

    assert_outcome_unknown(&task.await.unwrap());
}

#[tokio::test]
async fn write_project_file_malformed_success_payload_is_outcome_unknown() {
    let (runtime, project) = write_runtime("write-malformed").await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .write_project_file(
                    project,
                    "new.txt".to_string(),
                    "hello\n".to_string(),
                    None,
                    None,
                )
                .await
        }
    });

    let request = wait_for_patch_agent_request(&runtime, "write-malformed").await;
    complete_patch_agent_request(
        &runtime,
        "write-malformed",
        &request.request_id,
        0,
        "{}",
        "",
    )
    .await;

    assert_outcome_unknown(&task.await.unwrap());
}

#[tokio::test]
async fn write_project_file_trustworthy_results_preserve_exact_effect_state() {
    let (runtime, project) = write_runtime("write-effect").await;
    let success = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .write_project_file(
                    project,
                    "new.txt".to_string(),
                    "hello\n".to_string(),
                    None,
                    None,
                )
                .await
        }
    });
    let success_request = wait_for_patch_agent_request(&runtime, "write-effect").await;
    complete_patch_agent_request(
        &runtime,
        "write-effect",
        &success_request.request_id,
        0,
        &json!({
            "path": "new.txt",
            "created": true,
            "overwritten": false,
            "bytes_written": 6,
            "sha256": "a".repeat(64),
            "changed": true,
            "state_changed": true,
            "execution_state": "completed"
        })
        .to_string(),
        "",
    )
    .await;
    let success = success.await.unwrap();
    assert!(success.success, "{:?}", success.error);
    assert_eq!(success.output["changed"], true);
    assert_eq!(success.output["state_changed"], true);
    assert_eq!(success.output["execution_state"], "completed");

    let rejected = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .write_project_file(
                    project,
                    "blocked.txt".to_string(),
                    "replacement".to_string(),
                    None,
                    None,
                )
                .await
        }
    });
    let rejected_request = wait_for_patch_agent_request(&runtime, "write-effect").await;
    complete_patch_agent_request(
        &runtime,
        "write-effect",
        &rejected_request.request_id,
        0,
        &json!({
            "path": "blocked.txt",
            "created": false,
            "overwritten": false,
            "bytes_written": 0,
            "sha256": null,
            "changed": false,
            "state_changed": false,
            "execution_state": "not_started",
            "error": "file exists and overwrite is false"
        })
        .to_string(),
        "",
    )
    .await;
    let rejected = rejected.await.unwrap();
    assert!(!rejected.success);
    assert_eq!(rejected.output["changed"], false);
    assert_eq!(rejected.output["state_changed"], false);
    assert_eq!(rejected.output["execution_state"], "not_started");
    assert_ne!(rejected.output["error_kind"], "outcome_unknown");
}

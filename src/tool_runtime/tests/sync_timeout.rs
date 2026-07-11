//! Synchronous timeout contract for cargo_* and run_shell.

use super::support::*;
use crate::shell_protocol::{ShellAgentPollRequest, ShellClientCapabilities};
use crate::tool_runtime::helpers::{
    resolve_sync_timeout_secs, DEFAULT_CARGO_TIMEOUT_SECS, MAX_SYNC_TIMEOUT_SECS,
    MIN_SYNC_TIMEOUT_SECS,
};
use crate::tool_runtime::validation_events::validation_summary_for_session;
use crate::tool_runtime::{SessionMode, ToolCall, ToolResult};

fn assert_timeout_rejected(result: &ToolResult, tool_name: &str) {
    assert!(
        !result.success,
        "{tool_name} should reject out-of-range timeout"
    );
    assert_eq!(result.output["command_started"], false);
    assert_eq!(result.output["failure_kind"], "invalid_arguments");
    assert_eq!(result.output["tool_failure"], true);
    assert!(result.output["exit_code"].is_null());
    let error = result.error.as_deref().unwrap_or("");
    assert!(
        error.contains(tool_name),
        "error should name calling tool {tool_name}: {error}"
    );
    assert!(
        error.contains("timeout_secs")
            && error.contains(&MIN_SYNC_TIMEOUT_SECS.to_string())
            && error.contains(&MAX_SYNC_TIMEOUT_SECS.to_string()),
        "error should describe the 1..=120 range: {error}"
    );
    assert!(
        !error.to_ascii_lowercase().contains("runshell"),
        "error must not leak runShell implementation detail: {error}"
    );
    // "run_shell" is allowed only when it is the calling tool name.
    if tool_name != "run_shell" {
        assert!(
            !error.contains("run_shell"),
            "error must not leak run_shell implementation detail: {error}"
        );
    }
}

async fn assert_no_pending_shell_request(
    runtime: &crate::tool_runtime::ToolRuntime,
    client_id: &str,
) {
    let req = runtime
        .shell_clients
        .poll(ShellAgentPollRequest {
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .expect("poll should succeed");
    assert!(
        req.is_none(),
        "out-of-range timeout must not enqueue agent shell request: {req:?}"
    );
}

#[test]
fn resolve_sync_timeout_secs_rejects_out_of_range() {
    assert_eq!(
        resolve_sync_timeout_secs(None, DEFAULT_CARGO_TIMEOUT_SECS).unwrap(),
        DEFAULT_CARGO_TIMEOUT_SECS
    );
    assert_eq!(resolve_sync_timeout_secs(Some(1), 120).unwrap(), 1);
    assert_eq!(resolve_sync_timeout_secs(Some(120), 120).unwrap(), 120);
    assert!(resolve_sync_timeout_secs(Some(0), 120).is_err());
    assert!(resolve_sync_timeout_secs(Some(121), 120).is_err());
    assert!(resolve_sync_timeout_secs(Some(300), 120).is_err());
    assert!(resolve_sync_timeout_secs(Some(600), 60).is_err());
}

#[tokio::test]
async fn cargo_validation_tools_reject_timeout_above_120_before_enqueue() {
    let runtime = runtime_with_agent_project("sync-timeout-cargo");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "sync-timeout-cargo", None, caps).await;
    let project = agent_test_project_id("sync-timeout-cargo");

    for (tool_name, timeout) in [
        ("cargo_check", 121u64),
        ("cargo_check", 300),
        ("cargo_test", 121),
        ("cargo_test", 300),
        ("cargo_fmt", 121),
        ("cargo_fmt", 300),
    ] {
        let result = match tool_name {
            "cargo_check" => {
                runtime
                    .cargo_check(
                        project.clone(),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(timeout),
                    )
                    .await
            }
            "cargo_test" => {
                runtime
                    .cargo_test(
                        project.clone(),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(timeout),
                    )
                    .await
            }
            "cargo_fmt" => {
                runtime
                    .cargo_fmt(project.clone(), None, Some(true), Some(timeout))
                    .await
            }
            _ => unreachable!(),
        };
        assert_timeout_rejected(&result, tool_name);
        assert_no_pending_shell_request(&runtime, "sync-timeout-cargo").await;
    }
}

#[tokio::test]
async fn run_shell_rejects_timeout_above_120_before_enqueue() {
    let runtime = runtime_with_agent_project("sync-timeout-shell");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "sync-timeout-shell", None, caps).await;
    let project = agent_test_project_id("sync-timeout-shell");

    for timeout in [121u64, 300] {
        let result = runtime
            .run_shell(project.clone(), "echo hi".to_string(), Some(timeout), None)
            .await;
        assert_timeout_rejected(&result, "run_shell");
        assert_no_pending_shell_request(&runtime, "sync-timeout-shell").await;
    }
}

#[tokio::test]
async fn timeout_rejection_does_not_pollute_validation_summary() {
    let runtime = runtime_with_agent_project("sync-timeout-ledger");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "sync-timeout-ledger", None, caps).await;
    let project = agent_test_project_id("sync-timeout-ledger");
    let auth = auth_context(None, true);

    let session = runtime
        .dispatch_with_auth(
            ToolCall::StartSession {
                project: Some(project.clone()),
                title: Some("timeout contract".to_string()),
                mode: SessionMode::Normal,
                deny_write_tools: false,
                deny_shell_tools: false,
            },
            Some(&auth),
        )
        .await;
    assert!(session.success, "{:?}", session.error);
    let session_id = session.output["session_id"]
        .as_str()
        .or_else(|| session.output["session"]["session_id"].as_str())
        .expect("session id")
        .to_string();

    let rejected = runtime
        .dispatch_with_auth(
            ToolCall::CargoCheck {
                project: project.clone(),
                session_id: Some(session_id.clone()),
                cwd: None,
                all_targets: Some(true),
                all_features: None,
                no_default_features: None,
                features: None,
                package: None,
                timeout_secs: Some(300),
            },
            Some(&auth),
        )
        .await;
    assert_timeout_rejected(&rejected, "cargo_check");

    let summary_after_reject = runtime
        .sessions
        .summary(&session_id, Some(50))
        .expect("session summary");
    let validation_after_reject = validation_summary_for_session(&summary_after_reject);
    assert_eq!(validation_after_reject["available"], false);
    assert_eq!(validation_after_reject["status"], "not_run");
    assert_eq!(validation_after_reject["events_total"], 0);
    assert_eq!(validation_after_reject["historical_failures"]["count"], 0);

    // A subsequent valid cargo_check must pass and not be mixed by the reject.
    let check_task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let project = project.clone();
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
                        timeout_secs: Some(60),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "sync-timeout-ledger")
        .await
        .expect("valid cargo_check should enqueue");
    assert!(req.command.contains("cargo check"));
    complete_patch_agent_request(
        &runtime,
        "sync-timeout-ledger",
        &req.request_id,
        0,
        "Finished `dev` profile [unoptimized + debuginfo] target(s)\n",
        "",
    )
    .await;
    let check = check_task.await.unwrap();
    assert!(check.success, "{:?}", check.error);

    let summary = runtime
        .sessions
        .summary(&session_id, Some(50))
        .expect("session summary after success");
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["available"], true);
    assert_eq!(validation["status"], "passed");
    assert_eq!(validation["latest_status"], "passed");
    assert_eq!(validation["events_total"], 1);
    assert_eq!(validation["historical_failures"]["count"], 0);
    assert_eq!(validation["latest"]["tool_name"], "cargo_check");
    assert_eq!(validation["latest"]["success"], true);

    // Parameter rejection remains a normal tool failure event, not validation evidence.
    let finished = summary
        .events
        .iter()
        .filter(|e| e.kind == "tool_call_finished" && e.tool_name == "cargo_check")
        .collect::<Vec<_>>();
    assert_eq!(finished.len(), 2);
    let reject_event = finished
        .iter()
        .find(|e| e.status.as_deref() == Some("failed"))
        .expect("reject finished event");
    assert_eq!(
        reject_event.failure_kind.as_deref(),
        Some("invalid_arguments")
    );
    assert!(reject_event.validation_output_summary.is_none());
    assert!(reject_event.exit_code.is_none());
}

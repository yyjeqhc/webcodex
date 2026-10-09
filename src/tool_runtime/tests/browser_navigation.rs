use super::support::*;
use crate::runner_protocol::{RunnerCapabilities, RunnerPollRequest};
use crate::tool_runtime::kernel::{
    HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolTransport,
};
use crate::tool_runtime::permissions::{AuthorityMode, PermissionEvaluator};
use crate::tool_runtime::specialized::try_dispatch_specialized_gateway;
use serde_json::{json, Value};

#[tokio::test]
async fn control_browser_navigation_failure_preserves_completed_execution_and_observation_recovery()
{
    let client_id = "browser-navigation-failure";
    let runtime = test_runtime()
        .with_permission_evaluator(PermissionEvaluator::with_mode(AuthorityMode::TrustedAgent));
    let mut auth = open_auth_context();
    auth.scopes.push(crate::auth::SCOPE_BROWSER_CONTROL.into());
    register_agent_projects_for_auth(
        &runtime,
        client_id,
        &auth,
        RunnerCapabilities {
            browser_control: true,
            ..Default::default()
        },
        vec![],
    )
    .await;

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        async move {
            try_dispatch_specialized_gateway(
                &runtime,
                &ToolCallRequest {
                    tool_name: "control_browser".into(),
                    arguments: json!({
                        "action":"navigate", "client_id":client_id,
                        "browser_id":"browser_fixture", "page_id":"page_fixture",
                        "url":"https://example.test/private?token=PRIVATE_QUERY"
                    }),
                },
                ToolCallContext {
                    transport: ToolTransport::Api,
                    session_id: None,
                    auth: Some(&auth),
                    window: None,
                    record_oauth_scope_denials: true,
                    host_file_import_trust: HostFileImportTrust::Untrusted,
                },
            )
            .await
            .unwrap()
        }
    });
    let request = wait_for_runner_request_for_client(&runtime, client_id).await;
    assert_eq!(request.kind, "browser_navigate");
    let payload: Value = serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
    assert_eq!(payload["browser_id"], "browser_fixture");
    assert_eq!(payload["page_id"], "page_fixture");
    let message = "Browser navigation failed: net::ERR_CONNECTION_REFUSED";
    complete_patch_agent_request_for_instance(
        &runtime,
        client_id,
        &format!("inst-{client_id}"),
        &request.request_id,
        0,
        &json!({
            "ok":false, "execution_state":"completed",
            "error":{
                "kind":"navigation_failed", "message":message,
                "execution_state":"completed", "recovery_action":"snapshot"
            }
        })
        .to_string(),
        "",
    )
    .await;
    let outcome = task.await.unwrap();
    assert!(!outcome.success);
    let result = outcome.result.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "completed");
    assert_eq!(result.output["error_kind"], "navigation_failed");
    assert_eq!(result.output["message"], message);
    assert_eq!(result.error.as_deref(), Some(message));
    assert!(result.output.get("stability").is_none());
    let suggested = &result.output["recovery"]["suggested_call"];
    assert_eq!(suggested["tool"], "observe_browser");
    assert_eq!(
        suggested["arguments"],
        json!({
            "action":"snapshot", "client_id":client_id,
            "browser_id":"browser_fixture", "page_id":"page_fixture"
        })
    );
    assert!(!result.output.to_string().contains("PRIVATE_QUERY"));
    assert!(
        runtime
            .runner_registry
            .poll(RunnerPollRequest {
                client_id: client_id.into(),
                runner_instance_id: format!("inst-{client_id}"),
            })
            .await
            .unwrap()
            .is_none(),
        "a navigation failure must not enqueue a retry"
    );
}

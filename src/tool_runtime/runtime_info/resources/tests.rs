use super::*;
use crate::tool_runtime::ToolRuntime;

#[test]
fn resource_observation_is_bounded_and_explicitly_server_scoped() {
    let value = observe();
    assert_eq!(value["scope"], "server_process");
    assert_eq!(value["pid"], std::process::id());
    assert!(value["observed_at_ms"].is_i64());
    assert!(value.to_string().len() < 4096);
    #[cfg(not(target_os = "linux"))]
    assert_eq!(value["status"], "unsupported");
}

#[tokio::test]
async fn resource_diagnostics_only_attach_to_successful_explicit_full_status() {
    let runtime = ToolRuntime::new_for_tests();
    for (compact, summary_only) in [(true, false), (false, true), (true, true)] {
        let result = runtime
            .runtime_status_with_options(None, compact, summary_only, None)
            .await;
        assert!(result.success);
        assert!(result.output.get("resources").is_none());
        assert!(result.output.get("session_persistence").is_none());
    }
    let bootstrap = runtime.runtime_status(None).await;
    assert!(bootstrap.success);
    assert!(bootstrap.output.get("resources").is_none());
    let full = runtime
        .runtime_status_with_options(None, false, false, None)
        .await;
    assert!(full.success);
    assert_eq!(full.output["resources"]["scope"], "server_process");
    assert_eq!(full.output["session_persistence"]["mode"], "memory");
    let failed = runtime
        .runtime_status_with_options(None, false, false, Some("missing-runner".into()))
        .await;
    assert!(!failed.success);
    assert!(failed.output.get("resources").is_none());
    assert!(failed.output.get("session_persistence").is_none());
}

#[tokio::test]
async fn resource_diagnostics_public_focused_status_never_claims_remote_measurements() {
    use crate::runner_protocol::{RunnerCapabilities, RunnerRegisterRequest};
    use crate::tool_runtime::ToolCall;
    let runtime = ToolRuntime::new_for_tests();
    runtime
        .runner_registry
        .register(crate::test_support::current_runner_registration(
            RunnerRegisterRequest {
                computer_session_availability: None,
                client_id: "remote-fixture".into(),
                runner_instance_id: "fixture-instance".into(),
                runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
                display_name: None,
                owner: None,
                hostname: None,
                host_context: None,
                capabilities: RunnerCapabilities::default(),
                policy: None,
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                coding_agent_providers: None,
                coding_agent_inventory: None,
            },
        ))
        .await
        .unwrap();
    let result = runtime
        .dispatch(ToolCall::RuntimeStatus {
            compact: false,
            summary_only: false,
            client_id: Some("remote-fixture".into()),
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["focus"]["client_id"], "remote-fixture");
    assert_eq!(result.output["resources"]["scope"], "server_process");
    assert_eq!(result.output["resources"]["pid"], std::process::id());
    assert!(result.output["focus"].get("resources").is_none());
    assert_eq!(result.output["session_persistence"]["mode"], "memory");
}

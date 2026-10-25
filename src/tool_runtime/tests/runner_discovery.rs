use super::super::runtime_info::ListRunnersOptions;
use super::super::{ToolCall, ToolRuntime};
use super::support::*;
use crate::auth::AuthContext;
use crate::runner_protocol::{RunnerCapabilities, RunnerRegisterRequest};
use serde_json::{json, Value};

async fn register(
    runtime: &ToolRuntime,
    client_id: &str,
    project_count: usize,
    auth: Option<&AuthContext>,
) {
    runtime
        .runner_registry
        .register_with_auth(
            RunnerRegisterRequest {
                owner: auth.and_then(|auth| auth.username.clone()),
                display_name: Some(format!("Runner {client_id}")),
                job_concurrency_limit: Some(4),
                ..crate::test_support::runner_registration(
                    client_id,
                    &format!("inst-{client_id}"),
                    RunnerCapabilities { git: true, async_shell_jobs: true, ..Default::default() },
                )
            },
            auth.map(crate::test_support::runner_access).as_ref(),
        )
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        client_id,
        &format!("inst-{client_id}"),
        (0..project_count)
            .map(|index| registered_project(&format!("project-{index:03}"), &format!("/fixture/project-{index:03}")))
            .collect(),
    )
    .await;
}

fn call(arguments: Value) -> ToolCall {
    ToolCall::from_tool_name("list_runners", arguments).unwrap()
}

#[tokio::test]
async fn runner_discovery_default_is_sparse_with_211_projects() {
    let runtime = test_runtime();
    register(&runtime, "large", 211, None).await;
    let default = runtime.dispatch(call(json!({}))).await;
    let explicit_summary = runtime.dispatch(call(json!({"summary_only": true}))).await;
    let full = runtime.dispatch(call(json!({"summary_only": false, "include_projects": true}))).await;
    assert!(default.success && explicit_summary.success && full.success);
    eprintln!(
        "runner_discovery_211_bytes default={} explicit_summary={} detailed_projects={}",
        serde_json::to_vec(&default).unwrap().len(),
        serde_json::to_vec(&explicit_summary).unwrap().len(),
        serde_json::to_vec(&full).unwrap().len(),
    );
    let runner = &default.output["runners"][0];
    assert_eq!(runner["projects_count"], 211);
    for field in ["projects", "project_inventory", "capabilities", "build", "host_context", "policy", "shell_profiles"] {
        assert!(runner.get(field).is_none(), "default exposed {field}");
    }
    assert_eq!(default.output["count"], 1);
}

#[tokio::test]
async fn runner_discovery_default_bounds_fleet_and_keeps_operator_inventory() {
    let runtime = test_runtime();
    for index in (0..131).rev() {
        register(&runtime, &format!("runner-{index:03}"), 0, None).await;
    }
    let result = runtime.dispatch(call(json!({}))).await;
    assert!(result.success);
    assert_eq!(result.output["count"], 30);
    assert_eq!(result.output["matched_count"], 131);
    assert_eq!(result.output["truncated"], true);
    assert_eq!(result.output["next_offset"], 30);
    let operator = runtime.query_runner_inventory(None, &ListRunnersOptions {
        summary_only: true, ..Default::default()
    }).await.unwrap();
    assert_eq!(operator.runners.len(), 131, "Console's shared query remains explicitly complete");
}

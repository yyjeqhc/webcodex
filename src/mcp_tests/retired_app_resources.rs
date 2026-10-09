//! Removed resource IDs must not deliver a new template under a stale Host cache key.
use super::*;

#[tokio::test]
async fn retired_app_resources_fail_closed_instead_of_serving_current_templates() {
    let runtime = test_runtime();
    let retired = [
        "ui://webcodex/agent-continuation/v1",
        "ui://webcodex/agent-continuation/v10",
        "ui://webcodex/agent-continuation/v11",
        "ui://webcodex/agent-continuation/v12",
        "ui://webcodex/agent-continuation/v13",
        "ui://webcodex/agent-continuation/v14",
        "ui://webcodex/agent-continuation/v15",
        "ui://webcodex/agent-continuation/v16",
        "ui://webcodex/agent-continuation/v17",
        "ui://webcodex/agent-continuation/v2",
        "ui://webcodex/agent-continuation/v3",
        "ui://webcodex/agent-continuation/v4",
        "ui://webcodex/agent-continuation/v5",
        "ui://webcodex/agent-continuation/v6",
        "ui://webcodex/agent-continuation/v7",
        "ui://webcodex/agent-continuation/v8",
        "ui://webcodex/agent-continuation/v9",
        "ui://webcodex/changes/v1",
        "ui://webcodex/changes/v2",
        "ui://webcodex/changes/v3",
        "ui://webcodex/computer/v1",
        "ui://webcodex/computer/v10",
        "ui://webcodex/computer/v11",
        "ui://webcodex/computer/v2",
        "ui://webcodex/computer/v3",
        "ui://webcodex/computer/v4",
        "ui://webcodex/computer/v5",
        "ui://webcodex/computer/v6",
        "ui://webcodex/computer/v7",
        "ui://webcodex/computer/v8",
        "ui://webcodex/computer/v9",
        "ui://webcodex/goal-plan/v1",
        "ui://webcodex/goal-plan/v2",
        "ui://webcodex/goal-plan/v3",
        "ui://webcodex/goal-plan/v4",
        "ui://webcodex/goal-plan/v5",
        "ui://webcodex/goal-plan/v6",
        "ui://webcodex/job-terminal-continuation/v1",
        "ui://webcodex/pdf/v1",
        "ui://webcodex/pdf/v2",
        "ui://webcodex/result/v1",
        "ui://webcodex/result/v2",
        "ui://webcodex/result/v3",
        "ui://webcodex/work-result/v1",
        "ui://webcodex/work-result/v10",
        "ui://webcodex/work-result/v11",
        "ui://webcodex/work-result/v12",
        "ui://webcodex/work-result/v13",
        "ui://webcodex/work-result/v14",
        "ui://webcodex/work-result/v15",
        "ui://webcodex/work-result/v16",
        "ui://webcodex/work-result/v17",
        "ui://webcodex/work-result/v18",
        "ui://webcodex/work-result/v19",
        "ui://webcodex/work-result/v20",
        "ui://webcodex/work-result/v21",
        "ui://webcodex/work-result/v29",
        "ui://webcodex/workbench/v1",
        "ui://webcodex/work-result/v2",
        "ui://webcodex/work-result/v3",
        "ui://webcodex/work-result/v4",
        "ui://webcodex/work-result/v5",
        "ui://webcodex/work-result/v6",
        "ui://webcodex/work-result/v7",
        "ui://webcodex/work-result/v8",
        "ui://webcodex/work-result/v9",
    ];
    // Prove this is an enabled resource path, not blanket App denial.
    let current = handle_with_app_policy(
        &runtime,
        rpc(
            "resources/read",
            Some(json!(870)),
            mcp_2026_ui_params(json!({"uri": MCP_WORK_RESULT_UI_RESOURCE_URI})),
        ),
        None,
        true,
    )
    .await;
    assert!(matches!(current, McpOutcome::Ok(_)));
    for uri in retired {
        let result = handle_with_app_policy(
            &runtime,
            rpc(
                "resources/read",
                Some(json!(871)),
                mcp_2026_ui_params(json!({"uri": uri})),
            ),
            None,
            true,
        )
        .await;
        match result {
            McpOutcome::BadRequest(value) => assert_eq!(value["error"]["code"], -32602, "{uri}"),
            other => panic!("retired resource {uri} must not alias a current App: {other:?}"),
        }
    }
}

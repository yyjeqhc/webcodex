use super::*;

#[tokio::test]
async fn mcp_runner_discovery_default_211_projection_bytes() {
    let runtime = test_runtime();
    runtime.runner_registry.register(crate::test_support::runner_registration(
        "large", "inst-large", RunnerCapabilities {git: true, async_shell_jobs: true, ..Default::default()}
    )).await.unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry, "large", "inst-large",
        (0..211).map(|index| {
            let id = format!("project-{index:03}");
            RunnerProjectSummary {
                name: Some(id.clone()), path: format!("/fixture/{id}"), id,
                allow_patch: true, kind: Some("repo".into()), registration_source: None,
                description: None, hooks: Vec::new(), disabled: false, revision: None,
                root_fingerprint: None, lineage: None, git_branch: None, git_head: None,
                git_dirty: None, updated_at: 123, shell_profile: None,
            }
        }).collect(),
    ).await;
    let result = runtime.dispatch(
        crate::tool_runtime::ToolCall::from_tool_name("list_runners", json!({})).unwrap()
    ).await;
    let mut response = json!({"result":{
        "structuredContent": result,
        "content":[{"type":"text","text":"WebCodex tool completed successfully."}],
        "isError":false
    }});
    crate::mcp::cli::project_response(Some("list_runners"), &mut response);
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    eprintln!("mcp_runner_discovery_211_bytes json={} cli={}",
        serde_json::to_vec(&result).unwrap().len(), text.len());
    assert!(result.output["runners"][0].get("projects").is_none());
    assert!(response["result"].get("structuredContent").is_none());
    assert!(!text.contains("project-000"));
}

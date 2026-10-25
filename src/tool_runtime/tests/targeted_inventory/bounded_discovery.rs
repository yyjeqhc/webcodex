use super::*;

#[tokio::test]
async fn bounded_project_discovery_default_page_sizes() {
    for count in [0usize, 1, 30, 100, 101, 211] {
        let runtime = test_runtime();
        register_target_agent(&runtime, "bounded", large_fixture_projects(count), None).await;
        let result = runtime.dispatch(list_projects_call(None, None, None, None, false)).await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["matched_count"], count);
        assert_eq!(result.output["count"], count.min(30), "inventory size {count}");
        assert_eq!(result.output["truncated"], count > 30);
    }
}

#[tokio::test]
async fn bounded_project_discovery_payload_measurement() {
    let runtime = test_runtime();
    let mut projects = large_fixture_projects(210);
    projects.push(registered_project("webcodex", "/tmp/webcodex"));
    register_target_agent(&runtime, "bounded", projects, None).await;
    let full = runtime.dispatch(list_projects_call(None, None, None, None, false)).await;
    let summary = runtime.dispatch(list_projects_call(None, None, None, None, true)).await;
    let error = runtime.resolve_project_input_for_auth("webcodx", None).await.unwrap_err().into_tool_result();
    println!("PROJECT_BYTES canonical={} summary={} typo_error={}",
        serde_json::to_vec(&full).unwrap().len(),
        serde_json::to_vec(&summary).unwrap().len(),
        serde_json::to_vec(&error).unwrap().len());
    assert_eq!(summary.output["count"], 30);
    assert_eq!(error.output["candidates"].as_array().unwrap().len(), 5);
    assert_eq!(error.output["candidates"][0]["id"], "agent:bounded:webcodex");
}

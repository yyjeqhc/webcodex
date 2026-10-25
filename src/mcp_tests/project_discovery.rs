use super::*;
use crate::mcp::request_policy::McpSurface;

async fn call(
    runtime: &ToolRuntime,
    auth: &crate::auth::AuthContext,
    surface: McpSurface,
    tool: &str,
    arguments: Value,
) -> Value {
    let params = if surface == McpSurface::Cli {
        json!({"name":tool,"arguments":arguments})
    } else {
        adaptive_runtime_gateway_params(tool, arguments)
    };
    match super::super::handle_mcp_request_for_surface(
        runtime,
        rpc("tools/call", Some(json!(1)), params),
        Some(auth),
        McpProtocolEra::Legacy,
        HostFileImportTrust::Untrusted,
        None,
        None,
        None,
        true,
        true,
        None,
        surface,
        None,
    )
    .await
    {
        McpOutcome::Ok(value) => value,
        other => panic!("project discovery failed: {other:?}"),
    }
}

#[tokio::test]
async fn bounded_project_discovery_mcp_surfaces() {
    let runtime = test_runtime();
    let auth = crate::auth::shared_key_context("project-discovery-fixture");
    let access = crate::test_support::runner_access(&auth);
    runtime
        .runner_registry
        .register_with_auth(
            crate::test_support::runner_registration(
                "bounded",
                "bounded-instance",
                RunnerCapabilities::default(),
            ),
            Some(&access),
        )
        .await
        .unwrap();
    let projects = (0..211)
        .map(|index| {
            let id = if index == 210 {
                "webcodex".to_string()
            } else {
                format!("project-{index:04}")
            };
            RunnerProjectSummary {
                name: Some(format!("Large Fixture Project {index:04}")),
                path: format!("/tmp/large-fixture/{id}"),
                id,
                allow_patch: true,
                kind: Some("repo".into()),
                registration_source: None,
                description: Some("large-fixture".into()),
                hooks: Vec::new(),
                disabled: false,
                revision: Some("revision".into()),
                root_fingerprint: None,
                lineage: None,
                git_branch: None,
                git_head: None,
                git_dirty: None,
                updated_at: 1,
                shell_profile: None,
            }
        })
        .collect();
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        "bounded",
        "bounded-instance",
        projects,
    )
    .await;
    let adaptive = call(
        &runtime,
        &auth,
        McpSurface::Adaptive,
        "list_projects",
        json!({}),
    )
    .await;
    let cli = call(&runtime, &auth, McpSurface::Cli, "list_projects", json!({})).await;
    let error = call(
        &runtime,
        &auth,
        McpSurface::Cli,
        "read_files",
        json!({"project":"webcodx","items":[{"path":"README.md","limit":1}]}),
    )
    .await;
    let cli_text = cli["result"]["content"][0]["text"].as_str().unwrap();
    let error_text = error["result"]["content"][0]["text"].as_str().unwrap();
    println!(
        "PROJECT_MCP_BYTES adaptive={} cli_text={} typo_cli_text={}",
        serde_json::to_vec(&adaptive["result"]["structuredContent"])
            .unwrap()
            .len(),
        cli_text.len(),
        error_text.len()
    );
    assert_eq!(
        adaptive["result"]["structuredContent"]["output"]["count"],
        30
    );
    assert_eq!(
        adaptive["result"]["structuredContent"]["output"]["next_offset"],
        30
    );
    assert!(
        adaptive["result"]["structuredContent"]["output"]["projects"][0]
            .get("revision")
            .is_none()
    );
    assert!(cli["result"].get("structuredContent").is_none());
    assert!(cli_text.contains("next_offset: 30"), "{cli_text}");
    assert!(cli_text.contains("count: 30"), "{cli_text}");
    assert!(cli_text.contains("matched_count: 211"), "{cli_text}");
    assert!(error_text.len() < 6000, "{error_text}");
    assert_eq!(error["result"]["isError"], true);
    assert_eq!(error_text.matches("agent:bounded:webcodex").count(), 1);
    let full = call(
        &runtime,
        &auth,
        McpSurface::Adaptive,
        "list_projects",
        json!({"summary_only":false,"project":"agent:bounded:webcodex"}),
    )
    .await;
    assert_eq!(
        full["result"]["structuredContent"]["output"]["projects"][0]["revision"],
        "revision"
    );
    let full_cli = call(
        &runtime,
        &auth,
        McpSurface::Cli,
        "list_projects",
        json!({"summary_only":false,"project":"agent:bounded:webcodex"}),
    )
    .await;
    assert!(full_cli["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("revision: revision"));
    let second = call(
        &runtime,
        &auth,
        McpSurface::Adaptive,
        "list_projects",
        json!({"offset":30}),
    )
    .await;
    assert_eq!(
        second["result"]["structuredContent"]["output"]["projects"][0]["id"],
        "agent:bounded:project-0030"
    );
    let descriptor = crate::mcp::cli::tools_list(Some(&auth), false);
    let schema = &descriptor["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "list_projects")
        .unwrap()["inputSchema"];
    assert_eq!(schema["properties"]["summary_only"]["default"], true);
    assert!(schema["properties"].get("offset").is_some());
}

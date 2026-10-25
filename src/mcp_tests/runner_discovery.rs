use super::*;

#[tokio::test]
async fn mcp_runner_discovery_default_211_projection_bytes() {
    let runtime = test_runtime();
    runtime
        .runner_registry
        .register(crate::test_support::runner_registration(
            "large",
            "inst-large",
            RunnerCapabilities {
                git: true,
                async_shell_jobs: true,
                ..Default::default()
            },
        ))
        .await
        .unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        "large",
        "inst-large",
        (0..211)
            .map(|index| {
                let id = format!("project-{index:03}");
                RunnerProjectSummary {
                    name: Some(id.clone()),
                    path: format!("/fixture/{id}"),
                    id,
                    allow_patch: true,
                    kind: Some("repo".into()),
                    registration_source: None,
                    description: None,
                    hooks: Vec::new(),
                    disabled: false,
                    revision: None,
                    root_fingerprint: None,
                    lineage: None,
                    git_branch: None,
                    git_head: None,
                    git_dirty: None,
                    updated_at: 123,
                    shell_profile: None,
                }
            })
            .collect(),
    )
    .await;
    let result = runtime
        .dispatch(crate::tool_runtime::ToolCall::from_tool_name("list_runners", json!({})).unwrap())
        .await;
    let mut response = json!({"result":{
        "structuredContent": result,
        "content":[{"type":"text","text":"WebCodex tool completed successfully."}],
        "isError":false
    }});
    crate::mcp::cli::project_response(Some("list_runners"), &mut response);
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    eprintln!(
        "mcp_runner_discovery_211_bytes json={} cli={}",
        serde_json::to_vec(&result).unwrap().len(),
        text.len()
    );
    assert!(result.output["runners"][0].get("projects").is_none());
    assert!(response["result"].get("structuredContent").is_none());
    assert!(!text.contains("project-000"));
}
async fn runner_request(
    runtime: &ToolRuntime,
    arguments: Value,
    surface: crate::mcp::request_policy::McpSurface,
) -> Value {
    let outcome = super::super::handle_mcp_request_for_surface(
        runtime,
        rpc(
            "tools/call",
            Some(json!(1)),
            adaptive_runtime_gateway_params("list_runners", arguments),
        ),
        None,
        McpProtocolEra::Stateless2026,
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
    .await;
    match outcome {
        McpOutcome::Ok(mut value) => {
            // The HTTP adapter applies CLI text projection after canonical dispatch.
            // Match that boundary, as cli_call_for_test does for other fixtures.
            if surface == crate::mcp::request_policy::McpSurface::Cli {
                crate::mcp::cli::project_response(Some("list_runners"), &mut value);
            }
            value
        }
        other => panic!("unexpected Runner discovery outcome: {other:?}"),
    }
}

fn runner_cli_text(response: &Value) -> &str {
    let result = &response["result"];
    assert!(result.get("structuredContent").is_none(), "{response}");
    assert_eq!(result["isError"], false, "{response}");
    let content = result["content"].as_array().expect("CLI content blocks");
    assert_eq!(content.len(), 1, "{response}");
    assert_eq!(content[0]["type"], "text");
    content[0]["text"].as_str().expect("CLI text result")
}

fn assert_runner_cli_field(text: &str, field: &str, expected: Value) {
    if let Ok(structured) = serde_json::from_str::<Value>(text) {
        // The formatter may use its lossless canonical JSON fallback.
        assert_eq!(structured["output"][field], expected, "{text}");
    } else {
        let expected_line = format!("{field}: {expected}");
        assert!(text.lines().any(|line| line == expected_line), "{text}");
    }
}

#[tokio::test]
async fn mcp_runner_discovery_adaptive_and_cli_share_bounded_defaults_and_paging() {
    use crate::mcp::request_policy::McpSurface;
    let runtime = test_runtime();
    for index in (0..31).rev() {
        runtime
            .runner_registry
            .register(crate::test_support::runner_registration(
                &format!("runner-{index:03}"),
                &format!("inst-{index:03}"),
                RunnerCapabilities::default(),
            ))
            .await
            .unwrap();
    }
    let adaptive = runner_request(&runtime, json!({}), McpSurface::Adaptive).await;
    let output = &adaptive["result"]["structuredContent"]["output"];
    assert_eq!(output["count"], 30);
    assert_eq!(output["matched_count"], 31);
    assert_eq!(output["next_offset"], 30);
    assert!(output["runners"][0].get("capabilities").is_none());
    let cli = runner_request(&runtime, json!({}), McpSurface::Cli).await;
    let text = runner_cli_text(&cli);
    assert_runner_cli_field(text, "count", json!(30));
    assert_runner_cli_field(text, "matched_count", json!(31));
    assert_runner_cli_field(text, "next_offset", json!(30));
    assert!(
        text.contains("runner-000") && text.contains("runner-029"),
        "{text}"
    );
    assert!(!text.contains("runner-030"), "{text}");
    assert!(!text.contains("capabilities"), "{text}");
    let last = runner_request(&runtime, json!({"offset":30}), McpSurface::Adaptive).await;
    assert_eq!(
        last["result"]["structuredContent"]["output"]["runners"][0]["client_id"],
        "runner-030"
    );
    assert!(last["result"]["structuredContent"]["output"]["next_offset"].is_null());
    let cli_last = runner_request(&runtime, json!({"offset":30}), McpSurface::Cli).await;
    let text = runner_cli_text(&cli_last);
    assert_runner_cli_field(text, "count", json!(1));
    assert_runner_cli_field(text, "matched_count", json!(31));
    assert_runner_cli_field(text, "next_offset", Value::Null);
    assert!(text.contains("runner-030"), "{text}");
    assert!(!text.contains("runner-029"), "{text}");
}

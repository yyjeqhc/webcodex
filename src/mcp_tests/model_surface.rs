use super::*;

fn adaptive_direct_auth() -> crate::auth::AuthContext {
    let mut auth = crate::auth::shared_key_context("adaptive-direct-test");
    auth.scopes.extend([
        crate::auth::SCOPE_PLUGIN_INSPECT.to_string(),
        crate::auth::SCOPE_PLUGIN_INVOKE.to_string(),
        crate::auth::SCOPE_PLUGIN_MANAGE.to_string(),
    ]);
    auth
}

fn tool_names(value: &Value) -> Vec<&str> {
    value["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name"))
        .collect()
}

#[tokio::test]
async fn adaptive_tools_list_exposes_ranked_direct_tools_and_gateway() {
    let runtime = test_runtime();
    let auth = adaptive_direct_auth();
    let outcome = handle_mcp_request(
        &runtime,
        rpc("tools/list", Some(json!(60)), mcp_2026_params(json!({}))),
        Some(&auth),
    )
    .await;
    let McpOutcome::Ok(value) = outcome else {
        panic!("Adaptive tools/list must succeed");
    };
    let names = tool_names(&value);
    assert!(names.contains(&crate::mcp::tools::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME));
    #[cfg(feature = "experimental-code-mode")]
    {
        const MAX_EXPERIMENTAL_CODE_MODE_TOOL_BYTES: usize = 4 * 1024;
        let code_mode = value["result"]["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .find(|tool| tool["name"] == "code_mode_exec")
            .expect("experimental Code Mode feature must expose code_mode_exec directly");
        let code_mode_bytes = serde_json::to_vec(code_mode).unwrap().len();
        assert!(
            code_mode_bytes <= MAX_EXPERIMENTAL_CODE_MODE_TOOL_BYTES,
            "experimental code_mode_exec compact schema cost {code_mode_bytes} exceeded {MAX_EXPERIMENTAL_CODE_MODE_TOOL_BYTES} bytes"
        );
    }
    assert!(
        !names.contains(&"apply_patch"),
        "long-tail tool leaked direct"
    );
    for required in [
        "work_on_project",
        "read_files",
        "search_project_texts",
        "search_and_read",
        "apply_text_edits",
        "run_process",
        "run_script",
        "run_shell",
        "cargo_check",
        "cargo_test",
        "show_changes",
        "run_detached_process",
        "observe_jobs",
        "list_jobs",
        "wait_for_job_terminal",
        "stop_job",
        "skill_load",
        "run_skill_resource",
    ] {
        assert!(
            names.contains(&required),
            "missing Adaptive direct tool {required}"
        );
        assert!(
            crate::tool_runtime::tool_definition::is_adaptive_runtime_direct_tool(required),
            "{required} must derive direct admission from ToolDefinition rank"
        );
    }
}

#[tokio::test]
async fn long_tail_manifest_routes_through_call_runtime_tool() {
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(61)),
            mcp_2026_params(json!({
                "name": "tool_manifest",
                "arguments": {"tool_name": "apply_patch"}
            })),
        ),
        None,
    )
    .await;
    let McpOutcome::Ok(value) = outcome else {
        panic!("tool_manifest must succeed");
    };
    let output = &value["result"]["structuredContent"]["output"];
    assert_eq!(output["route"]["mode"], "gateway");
    assert_eq!(output["route"]["via"], "call_runtime_tool");
}

#[tokio::test]
async fn closeout_helpers_remain_visible_with_exact_gateway_contracts() {
    let runtime = test_runtime();
    for name in ["workspace_hygiene_check", "finish_coding_task"] {
        let definition =
            crate::tool_runtime::tool_definition::lookup_tool_definition(name).unwrap();
        assert!(definition.visibility.is_model_visible());
        assert_eq!(definition.adaptive_runtime_direct_rank(), None);
        let listed = crate::mcp::tools::mcp_tools_list_payload_with_compact(false);
        assert!(!listed["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == name));
        let McpOutcome::Ok(value) = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(json!(66)),
                mcp_2026_params(json!({
                    "name": "tool_manifest", "arguments": {"tool_name": name},
                })),
            ),
            None,
        )
        .await
        else {
            panic!("manifest {name}");
        };
        let output = &value["result"]["structuredContent"]["output"];
        assert_eq!(
            output["route"],
            json!({"mode": "gateway", "via": "call_runtime_tool"})
        );
        assert_eq!(
            output["input_schema"],
            webcodex_tool_contracts::input_schema_for_tool(name)
        );
        assert_eq!(output["effect"], "observe");
        assert!(crate::mcp::tools::adaptive_runtime_gateway_target_admitted_for_test(name, true));
    }
}

#[tokio::test]
async fn long_tail_direct_call_is_rejected_with_gateway_guidance() {
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(62)),
            mcp_2026_params(json!({
                "name": "apply_patch",
                "arguments": {
                    "project": "missing-project",
                    "patch": "*** Begin Patch\n*** Add File: route-probe.txt\n+probe\n*** End Patch",
                    "dry_run": true
                }
            })),
        ),
        None,
    )
    .await;
    let McpOutcome::BadRequest(value) = outcome else {
        panic!("direct long-tail invocation must be rejected");
    };
    let message = value["error"]["message"].as_str().unwrap();
    assert!(message.contains("call_runtime_tool"), "{message}");
    assert!(
        message.contains("not directly callable on Adaptive Runtime"),
        "{message}"
    );
}

#[tokio::test]
async fn long_tail_and_direct_targets_are_both_admitted_through_gateway() {
    let runtime = test_runtime();
    for (id, tool, arguments) in [
        (
            63,
            "apply_patch",
            json!({
                "project": "missing-project",
                "patch": "*** Begin Patch\n*** Add File: route-probe.txt\n+probe\n*** End Patch",
                "dry_run": true
            }),
        ),
        (
            64,
            "read_files",
            json!({"project": "missing-project", "items": [{"path": "x"}]}),
        ),
    ] {
        let outcome = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(json!(id)),
                mcp_2026_params(json!({
                    "name": "call_runtime_tool",
                    "arguments": {"tool": tool, "arguments": arguments}
                })),
            ),
            None,
        )
        .await;
        assert!(
            matches!(outcome, McpOutcome::Ok(_)),
            "gateway must admit canonical target {tool}: {outcome:?}"
        );
    }
}

#[test]
fn hidden_protocol_extensions_require_protocol_admission() {
    assert!(!crate::tool_runtime::tool_definition::is_model_visible_tool_name("skill_list"));
    assert!(
        !crate::mcp::tools::adaptive_runtime_gateway_target_admitted_for_test("skill_list", false,)
    );
    assert!(
        crate::mcp::tools::adaptive_runtime_gateway_target_admitted_for_test("skill_list", true,)
    );
    assert!(
        !crate::mcp::tools::adaptive_runtime_gateway_target_admitted_for_test(
            "definitely_unknown_tool",
            true,
        )
    );
}

#[tokio::test]
async fn call_runtime_tool_cannot_target_itself() {
    let runtime = test_runtime();
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(65)),
            mcp_2026_params(json!({
                "name": "call_runtime_tool",
                "arguments": {
                    "tool": "call_runtime_tool",
                    "arguments": {"tool": "read_files", "arguments": {}}
                }
            })),
        ),
        None,
    )
    .await;
    let McpOutcome::BadRequest(value) = outcome else {
        panic!("recursive gateway call must be rejected");
    };
    assert!(value["error"]["message"]
        .as_str()
        .unwrap()
        .contains("cannot target itself"));
}

#[tokio::test]
async fn call_runtime_tool_rejects_direct_app_presentation_targets_when_apps_are_enabled() {
    let runtime = test_runtime();
    for (index, target) in [
        "present_work_result",
        "present_goal_plan",
        "present_agent_continuation",
        "present_job_terminal_continuation",
    ]
    .into_iter()
    .enumerate()
    {
        let request = rpc(
            "tools/call",
            Some(json!(70 + index)),
            mcp_2026_ui_params(adaptive_runtime_gateway_params(target, json!({}))),
        );
        let protocol_era = super::super::inferred_protocol_era(&request);
        let outcome = super::super::handle_mcp_request_with_lifecycle(
            &runtime,
            request,
            None,
            protocol_era,
            super::super::HostFileImportTrust::Untrusted,
            None,
            None,
            None,
            crate::model_surface::effective_mcp_compact_schemas(
                crate::config::mcp_compact_schemas_override(),
            ),
            true,
            None,
        )
        .await;
        let McpOutcome::BadRequest(value) = outcome else {
            panic!("{target} must require its direct MCP App presentation route");
        };
        let message = value["error"]["message"].as_str().unwrap();
        assert!(message.contains("call_runtime_tool cannot invoke MCP App presentation tool"));
        assert!(message.contains(target));
        assert!(message.contains("directly"));
    }
}

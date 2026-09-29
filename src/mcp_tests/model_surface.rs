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
async fn model_workflow_policy_changes_never_change_cached_tools_schemas_routes_or_apps() {
    use crate::model_workflow::ModelWorkflowPolicy;
    let auth = adaptive_direct_auth();
    let full_schema_baseline = serde_json::to_vec(
        &crate::mcp::tools::mcp_tools_list_payload_with_compact(false),
    )
    .unwrap();
    for apps in [false, true] {
        let mut listed_baseline = None;
        let mut manifest_baseline = None;
        for preference in ["on_demand", "preferred"] {
            for mode in ["unknown", "user_confirmed", "unattended"] {
                let runtime = test_runtime().with_model_workflow_policy(
                    ModelWorkflowPolicy::from_values(Some(preference), Some(mode)).unwrap(),
                );
                let params = if apps {
                    mcp_2026_ui_params(json!({}))
                } else {
                    mcp_2026_params(json!({}))
                };
                let McpOutcome::Ok(listed) = handle_mcp_request(
                    &runtime,
                    rpc("tools/list", Some(json!(160)), params),
                    Some(&auth),
                )
                .await
                else {
                    panic!("tools/list");
                };
                let bytes = serde_json::to_vec(&listed["result"]).unwrap();
                if let Some(expected) = &listed_baseline {
                    assert_eq!(&bytes, expected);
                } else {
                    listed_baseline = Some(bytes);
                }
                assert_eq!(
                    serde_json::to_vec(&crate::mcp::tools::mcp_tools_list_payload_with_compact(
                        false
                    ))
                    .unwrap(),
                    full_schema_baseline
                );
                let mut manifests = Vec::new();
                for tool in [
                    "present_goal_plan",
                    "prepare_goal_workflow",
                    "checkpoint_goal",
                    "get_goal",
                    "update_goal",
                    "present_agent_continuation",
                ] {
                    let args = json!({"name":"tool_manifest", "arguments":{"tool_name":tool}});
                    let params = if apps {
                        mcp_2026_ui_params(args)
                    } else {
                        mcp_2026_params(args)
                    };
                    let McpOutcome::Ok(manifest) = handle_mcp_request(
                        &runtime,
                        rpc("tools/call", Some(json!(161)), params),
                        Some(&auth),
                    )
                    .await
                    else {
                        panic!("manifest {tool}");
                    };
                    manifests.push(manifest["result"].clone());
                }
                if let Some(expected) = &manifest_baseline {
                    assert_eq!(&manifests, expected);
                } else {
                    manifest_baseline = Some(manifests);
                }
            }
        }
    }
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
    for specialist in webcodex_tool_contracts::EXACT_DISCOVERY_SPECIALIST_TOOL_NAMES {
        assert!(
            !names.contains(specialist),
            "{specialist} must stay off the ordinary Adaptive direct surface"
        );
        assert!(
            !crate::tool_runtime::tool_definition::is_adaptive_runtime_direct_tool(specialist),
            "{specialist} must route through exact discovery/gateway"
        );
    }
    for required in [
        "work_on_project",
        "read_files",
        "search_project_texts",
        "search_and_read",
        "edit_project_files",
        "run_process",
        "run_script",
        "run_shell",
        "cargo_check",
        "cargo_test",
        "review_changes",
        "observe_jobs",
        "wait_for_job_readiness",
        "present_work_result",
        "present_goal_plan",
        "skill_load",
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
async fn specialist_tools_remain_discoverable_with_canonical_gateway_contracts() {
    let runtime = test_runtime();
    let listed = crate::mcp::tools::mcp_tools_list_payload_with_compact(false);
    for name in [
        "show_changes",
        "session_handoff_summary",
        "rotate_agent_continuation_endpoint",
        "run_skill_resource",
        "wait_for_agent_events",
        "wait_for_job_terminal",
    ] {
        let definition = webcodex_tool_contracts::lookup_tool_definition(name).unwrap();
        assert!(definition.visibility.is_model_visible());
        assert_eq!(definition.adaptive_runtime_direct_rank(), None);
        assert!(!listed["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == name));
        let McpOutcome::Ok(value) = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(json!(67)),
                mcp_2026_params(json!({
                    "name": "tool_manifest", "arguments": {"tool_name": name}
                })),
            ),
            None,
        )
        .await
        else {
            panic!("manifest {name}");
        };
        let output = &value["result"]["structuredContent"]["output"];
        assert_eq!(output["route"]["primary"]["mode"], "gateway", "{name}");
        assert_eq!(output["route"]["primary"]["tool"], "call_runtime_tool");
        assert_eq!(output["route"]["primary"]["target"], name);
        assert_eq!(
            output["input_schema"],
            webcodex_tool_contracts::input_schema_for_tool(name)
        );
        assert_eq!(
            output["effect"],
            definition.metadata().effect.manifest_label()
        );
        assert_eq!(
            output["idempotency"],
            definition.metadata().idempotency.manifest_label()
        );
        assert!(crate::mcp::tools::adaptive_runtime_gateway_target_admitted_for_test(name, true));
    }
}

#[tokio::test]
async fn inactive_continuation_presentations_are_unavailable_not_gateway_tools() {
    let runtime = test_runtime();
    for apps in [false, true] {
        for name in [
            "present_agent_continuation",
            "present_job_terminal_continuation",
        ] {
            assert_eq!(
                crate::model_surface::suggested_tool_call_route(name, false),
                crate::model_surface::SuggestedToolCallRoute::Unavailable
            );
            assert!(
                !crate::mcp::tools::adaptive_runtime_gateway_target_admitted_for_test(name, true)
            );
            let listed = crate::mcp::tools::mcp_tools_list_payload_with_features_for_auth(
                false, apps, true, None,
            );
            assert!(!listed["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["name"] == name));
            for (gateway, params) in [
                (false, json!({"name": name, "arguments": {}})),
                (true, adaptive_runtime_gateway_params(name, json!({}))),
            ] {
                let params = if apps {
                    mcp_2026_ui_params(params)
                } else {
                    mcp_2026_params(params)
                };
                let outcome =
                    handle_mcp_request(&runtime, rpc("tools/call", Some(json!(68)), params), None)
                        .await;
                if gateway {
                    let McpOutcome::Ok(value) = outcome else {
                        panic!("{name}: {outcome:?}")
                    };
                    let result = &value["result"]["structuredContent"];
                    assert_eq!(result["success"], false);
                    assert_eq!(result["output"]["error_kind"], "unknown_tool");
                    assert_eq!(result["output"]["execution_state"], "not_started");
                    assert_eq!(result["output"]["state_changed"], false);
                    assert!(result["output"].get("suggested_call").is_none());
                } else {
                    assert!(
                        matches!(outcome, McpOutcome::BadRequest(_)),
                        "{name}: {outcome:?}"
                    );
                }
            }
        }
    }
}

#[cfg(not(feature = "legacy-gpt-actions"))]
#[tokio::test]
async fn retired_endpoint_name_is_absent_from_adaptive_exact_discovery() {
    let runtime = test_runtime();
    let old = "attach_agent_endpoint";
    assert!(webcodex_tool_contracts::lookup_tool_definition(old).is_none());
    assert_eq!(
        crate::model_surface::suggested_tool_call_route(old, true),
        crate::model_surface::SuggestedToolCallRoute::Unavailable
    );
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(69)),
            mcp_2026_params(json!({
                "name":"tool_manifest", "arguments":{"tool_name":old}
            })),
        ),
        None,
    )
    .await;
    let McpOutcome::Ok(value) = outcome else {
        panic!("exact manifest: {outcome:?}")
    };
    let result = &value["result"]["structuredContent"];
    assert_eq!(result["success"], false);
    assert_eq!(result["output"]["code"], "unknown_tool_manifest_tool");
    assert!(result["output"].get("tools").is_none());
    assert!(result["output"].get("route").is_none());
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
    assert_eq!(output["route"]["primary"]["mode"], "gateway");
    assert_eq!(output["route"]["primary"]["tool"], "call_runtime_tool");
    assert_eq!(output["route"]["primary"]["target"], "apply_patch");
    assert!(output["route"]["fallback"].is_null());
    assert_eq!(output["route"]["tool_manifest_registers_host_tool"], false);
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
        assert_eq!(output["route"]["primary"]["mode"], "gateway");
        assert_eq!(output["route"]["primary"]["tool"], "call_runtime_tool");
        assert_eq!(output["route"]["primary"]["target"], name);
        assert!(output["route"]["fallback"].is_null());
        assert_eq!(output["route"]["tool_manifest_registers_host_tool"], false);
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
    for (index, target) in ["present_work_result", "present_goal_plan"]
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

#[tokio::test]
async fn pruned_tools_keep_exact_manifest_and_canonical_gateway_validation() {
    let runtime = test_runtime();
    for (name, arguments) in [
        ("list_jobs", json!({})),
        (
            "wait_for_job_terminal",
            json!({"job_id":"missing", "idempotency_key":"surface-parity"}),
        ),
        (
            "wait_for_agent_events",
            json!({
                "agent_id":"wc_dagent_qqqqqqqqqqqqqqqq",
                "endpoint_id":"wc_endpoint_qqqqqqqqqqqqqqqq",
                "expected_controller_generation":1,
                "events":[{"kind":"agent_task_terminal", "task_id":"wc_agent_task_qqqqqqqqqqqqqqqq"}],
                "idempotency_key":"surface-parity"
            }),
        ),
        (
            "stop_job",
            json!({"project": "missing", "job_id": "missing", "confirm": true}),
        ),
        (
            "run_detached_process",
            json!({"project": "missing", "executable": "missing", "idempotency_key": "surface-parity"}),
        ),
        (
            "transfer_project_artifact",
            json!({"source_project": "missing", "source_path": "a", "destination_project": "missing", "destination_path": "b"}),
        ),
    ] {
        assert!(!webcodex_tool_contracts::is_adaptive_runtime_direct_tool(
            name
        ));
        let McpOutcome::Ok(manifest) = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(json!(1)),
                mcp_2026_params(json!({"name": "tool_manifest", "arguments": {"tool_name": name}})),
            ),
            None,
        )
        .await
        else {
            panic!("manifest {name}")
        };
        let output = &manifest["result"]["structuredContent"]["output"];
        assert_eq!(output["route"]["primary"]["mode"], "gateway");
        assert_eq!(output["route"]["primary"]["target"], name);
        let call = crate::tool_runtime::ToolCall::from_tool_name(name, arguments.clone()).unwrap();
        let canonical = runtime.dispatch(call).await;
        let McpOutcome::Ok(result) = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(json!(2)),
                mcp_2026_params(adaptive_runtime_gateway_params(name, arguments.clone())),
            ),
            None,
        )
        .await
        else {
            panic!("gateway {name}")
        };
        let actual = &result["result"]["structuredContent"];
        assert_eq!(actual["success"], canonical.success, "{name}");
        assert_eq!(
            actual["output"]["error_kind"], canonical.output["error_kind"],
            "{name}"
        );
        let mut invalid = arguments;
        invalid["unknown_business_field"] = json!(true);
        assert!(crate::tool_runtime::ToolCall::from_tool_name(name, invalid.clone()).is_err());
        let result = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(json!(3)),
                mcp_2026_params(adaptive_runtime_gateway_params(name, invalid)),
            ),
            None,
        )
        .await;
        match result {
            McpOutcome::Ok(value) => {
                assert_eq!(value["result"]["structuredContent"]["success"], false)
            }
            McpOutcome::BadRequest(_) => {}
            other => panic!("invalid {name}: {other:?}"),
        }
    }
}

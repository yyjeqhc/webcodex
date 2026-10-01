use super::*;

const JOB_APP_TOOLS: [&str; 5] = [
    "bind_job_terminal_continuation",
    "get_job_terminal_continuation_state",
    "prepare_job_terminal_continuation",
    "finish_job_terminal_continuation",
    "unbind_job_terminal_continuation",
];

fn tool<'a>(payload: &'a Value, name: &str) -> Option<&'a Value> {
    payload["tools"]
        .as_array()?
        .iter()
        .find(|tool| tool["name"] == name)
}

fn job_app_auth() -> crate::auth::AuthContext {
    let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::ApiToken);
    auth.user_id = Some("user-job-app".to_string());
    auth.username = Some("job-app".to_string());
    auth.api_key_id = Some("key-job-app".to_string());
    auth.role = Some("user".to_string());
    auth.scopes = vec![crate::auth::SCOPE_RUNTIME_READ.to_string()];
    auth.token_kind = Some("user".to_string());
    auth
}

async fn handle_with_apps(
    runtime: &ToolRuntime,
    request: JsonRpcRequest,
    auth: Option<&crate::auth::AuthContext>,
    enabled: bool,
) -> McpOutcome {
    let protocol_era = super::super::inferred_protocol_era(&request);
    let window = crate::client_window::stateless_mcp_window(&request.params);
    super::super::handle_mcp_request_with_lifecycle(
        runtime,
        request,
        auth,
        protocol_era,
        super::super::HostFileImportTrust::Untrusted,
        window.identity.as_ref(),
        None,
        None,
        crate::model_surface::effective_mcp_compact_schemas(
            crate::config::mcp_compact_schemas_override(),
        ),
        enabled,
        None,
    )
    .await
}

#[test]
fn job_terminal_wait_does_not_suggest_an_unadvertised_host_carrier() {
    let base = json!({
        "wait_id": "wc_job_wait_q6urq6urq6urq6ur",
        "job_id": "wc_job_exact",
        "state": "waiting",
        "delivery_state": "not_ready",
        "terminal_status": null,
        "terminal_outcome": null,
        "replayed": false,
        "state_changed": true,
        "automatic_resume_available": false,
        "expires_at": 4_102_444_800_i64,
        "fallback_tool": "observe_jobs",
    });

    let mut capable = ToolResult::ok(base.clone());
    super::super::tools::project_job_terminal_resume_suggested_call(true, &mut capable);
    assert_eq!(
        capable.output, base,
        "hidden carrier must not create a dead follow-up"
    );
    crate::tool_runtime::ToolCall::from_tool_name(
        "present_job_terminal_continuation",
        json!({"wait_id": "wc_job_wait_q6urq6urq6urq6ur"}),
    )
    .expect("hidden domain ToolCall is retained");

    let mut no_carrier = ToolResult::ok(base.clone());
    super::super::tools::project_job_terminal_resume_suggested_call(false, &mut no_carrier);
    assert!(no_carrier.output.get("suggested_call").is_none());

    let mut already_bound = ToolResult::ok({
        let mut value = base.clone();
        value["automatic_resume_available"] = json!(true);
        value
    });
    super::super::tools::project_job_terminal_resume_suggested_call(true, &mut already_bound);
    assert!(already_bound.output.get("suggested_call").is_none());

    let mut triggered = ToolResult::ok({
        let mut value = base.clone();
        value["state"] = json!("triggered");
        value["delivery_state"] = json!("pending");
        value["terminal_status"] = json!("completed");
        value["terminal_outcome"] = json!("succeeded");
        value
    });
    super::super::tools::project_job_terminal_resume_suggested_call(true, &mut triggered);
    assert!(
        triggered.output.get("suggested_call").is_none(),
        "already-triggered terminal truth belongs to the current model turn"
    );

    let mut unknown = ToolResult::ok({
        let mut value = base;
        value["delivery_state"] = json!("delivery_unknown");
        value
    });
    super::super::tools::project_job_terminal_resume_suggested_call(true, &mut unknown);
    assert!(unknown.output.get("suggested_call").is_none());
}

#[tokio::test]
async fn job_terminal_continuation_app_surface_is_explicit_sparse_and_app_only() {
    assert_eq!(
        MCP_JOB_TERMINAL_CONTINUATION_UI_RESOURCE_URI,
        "ui://webcodex/job-terminal-continuation/v2"
    );
    let runtime = ToolRuntime::new_for_tests();
    let auth = job_app_auth();
    let ui = handle_with_apps(
        &runtime,
        rpc(
            "tools/list",
            Some(json!(6101)),
            mcp_2026_ui_params(json!({})),
        ),
        Some(&auth),
        true,
    )
    .await;
    let McpOutcome::Ok(ui) = ui else {
        panic!("expected UI tools/list")
    };

    assert!(tool(&ui["result"], "present_job_terminal_continuation").is_none());
    assert!(tool(&ui["result"], "wait_for_job_terminal").is_none());
    for apps in [false, true] {
        let full = super::super::tools::mcp_tools_list_payload_with_features_for_auth(
            false,
            apps,
            true,
            Some(&auth),
        );
        assert!(tool(&full, "wait_for_job_terminal").is_none());
        assert!(tool(&full, "present_job_terminal_continuation").is_none());
    }
    for name in JOB_APP_TOOLS {
        let descriptor = tool(&ui["result"], name).unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(
            descriptor.pointer("/_meta/ui/visibility"),
            Some(&json!(["app"]))
        );
        assert!(descriptor.pointer("/_meta/ui/resourceUri").is_none());
        assert_eq!(
            descriptor.pointer("/inputSchema/properties/app_call_id/pattern"),
            Some(&json!("^wc_app_call_[0-9a-f]{16}_[1-9][0-9]{0,5}$"))
        );
        assert!(!descriptor["inputSchema"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == "app_call_id"));
        assert!(
            !super::super::tools::adaptive_runtime_gateway_target_admitted_for_test(name, true),
            "{name} must not be exposed through the generic Adaptive gateway"
        );
    }

    let plain = handle_with_apps(
        &runtime,
        rpc("tools/list", Some(json!(6102)), mcp_2026_params(json!({}))),
        Some(&auth),
        true,
    )
    .await;
    let McpOutcome::Ok(plain) = plain else {
        panic!("expected plain tools/list")
    };
    assert!(tool(&plain["result"], "present_job_terminal_continuation").is_none());
    for name in JOB_APP_TOOLS {
        assert!(tool(&plain["result"], name).is_none());
    }

    let disabled = handle_with_apps(
        &runtime,
        rpc(
            "tools/list",
            Some(json!(6103)),
            mcp_2026_ui_params(json!({})),
        ),
        Some(&auth),
        false,
    )
    .await;
    let McpOutcome::Ok(disabled) = disabled else {
        panic!("expected disabled tools/list")
    };
    assert!(tool(&disabled["result"], "present_job_terminal_continuation").is_none());
    for name in JOB_APP_TOOLS {
        assert!(tool(&disabled["result"], name).is_none());
    }

    let resources = handle_with_apps(
        &runtime,
        rpc(
            "resources/list",
            Some(json!(6104)),
            mcp_2026_ui_params(json!({})),
        ),
        Some(&auth),
        true,
    )
    .await;
    let McpOutcome::Ok(resources) = resources else {
        panic!("expected resources/list")
    };
    let resource = resources["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|resource| resource["uri"] == MCP_JOB_TERMINAL_CONTINUATION_UI_RESOURCE_URI)
        .expect("Job continuation resource");
    assert_eq!(resource["mimeType"], MCP_UI_RESOURCE_MIME_TYPE);

    let read = handle_with_apps(
        &runtime,
        rpc(
            "resources/read",
            Some(json!(6105)),
            mcp_2026_ui_params(json!({
                "uri": MCP_JOB_TERMINAL_CONTINUATION_UI_RESOURCE_URI
            })),
        ),
        Some(&auth),
        true,
    )
    .await;
    let McpOutcome::Ok(read) = read else {
        panic!("expected Job continuation resource read")
    };
    assert_eq!(
        read["result"]["contents"][0]["text"],
        MCP_JOB_TERMINAL_CONTINUATION_APP_HTML
    );
}

#[tokio::test]
async fn job_terminal_hidden_kernel_entry_is_fail_closed_without_protocol_capability() {
    use crate::tool_runtime::kernel::{
        HostFileImportTrust, ToolCallContext, ToolCallErrorStatus, ToolCallRequest,
        ToolProtocolCapabilities, ToolTransport,
    };

    let runtime = ToolRuntime::new_for_tests();
    let auth = job_app_auth();
    let wait_id = "wc_job_wait_q6urq6urq6urq6ur";
    let binding_id = "wc_host_binding_qqqqqqqqqqqqqqqqqqqqqg";
    let attempt_id = "wc_job_delivery_ZmZmZmZmZmZmZmZm";
    for transport in [ToolTransport::Mcp, ToolTransport::Api] {
        for name in JOB_APP_TOOLS {
            let mut arguments = json!({
                "wait_id": wait_id,
                "binding_id": binding_id,
            });
            if name == "finish_job_terminal_continuation" {
                arguments["attempt_id"] = json!(attempt_id);
                arguments["outcome"] = json!("dispatch_accepted");
            }
            let outcome = runtime
                .call_tool_with_protocol_capabilities(
                    ToolCallRequest {
                        tool_name: name.to_string(),
                        arguments,
                    },
                    ToolCallContext {
                        transport,
                        session_id: None,
                        auth: Some(&auth),
                        window: None,
                        record_oauth_scope_denials: false,
                        host_file_import_trust: HostFileImportTrust::Untrusted,
                    },
                    ToolProtocolCapabilities::default(),
                )
                .await;
            assert!(matches!(
                outcome.error_status,
                Some(ToolCallErrorStatus::InvalidArguments { ref message })
                    if message.contains("Job terminal continuation App coordination")
            ));
            assert!(outcome.result.is_none());
        }
    }
}

#[test]
fn job_terminal_continuation_app_source_encodes_bounded_pull_and_single_dispatch_fence() {
    for required in [
        "ui/initialize",
        "ui/notifications/tool-input",
        "bind_job_terminal_continuation",
        "get_job_terminal_continuation_state",
        "prepare_job_terminal_continuation",
        "ui/message",
        "finish_job_terminal_continuation",
        "unbind_job_terminal_continuation",
        "visibilitychange",
        "pagehide",
        "beforeunload",
        "ui/resource-teardown",
        "delivery_unknown",
        "VISIBLE_POLL_MS = 3000",
        "HIDDEN_EARLY_POLL_MS = 15000",
        "HIDDEN_MEDIUM_POLL_MS = 60000",
        "HIDDEN_LATE_POLL_MS = 300000",
        "HIDDEN_EARLY_POLLS = 20",
        "HIDDEN_MEDIUM_POLLS = 25",
        "AUTO_RESUME_TURN_YIELD_GRACE_MS = 10000",
    ] {
        assert!(
            MCP_JOB_TERMINAL_CONTINUATION_APP_HTML.contains(required),
            "missing {required}"
        );
    }
    assert_eq!(
        MCP_JOB_TERMINAL_CONTINUATION_APP_HTML
            .matches("request(\"ui/message\"")
            .count(),
        1,
        "the shipped View must have one Host dispatch site"
    );
    for forbidden in [
        "callTool(\"observe_jobs\"",
        "localStorage",
        "console.log",
        "wc_peer_",
        "openai/session",
        "Authorization",
    ] {
        assert!(
            !MCP_JOB_TERMINAL_CONTINUATION_APP_HTML.contains(forbidden),
            "App source contains forbidden marker {forbidden}"
        );
    }
    assert!(MCP_JOB_TERMINAL_CONTINUATION_APP_HTML.contains("dispatchStartedAttempt = attemptId;"));
    assert!(MCP_JOB_TERMINAL_CONTINUATION_APP_HTML
        .contains("current.projection.delivery_state === \"prepared\""));
    assert!(MCP_JOB_TERMINAL_CONTINUATION_APP_HTML
        .contains("await finish(current.preparedAttemptId, \"delivery_unknown\")"));
}

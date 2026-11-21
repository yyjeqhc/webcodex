//! Actual Server adapter/Runner receipt contract, not a claim about a live
//! ChatGPT/Codex client's deferred tool loader or Apps cache.
use super::*;

#[tokio::test]
async fn client_contract_edit_success_keeps_effects_and_safe_numeric_fences_across_routes() {
    for (modern, client) in [
        (false, "generic-test-client"),
        (true, "generic-test-client"),
        (true, "openai-mcp"),
    ] {
        for gateway in [false, true] {
            for dry_run in [false, true] {
                let (_tmp, db) = test_db();
                let runtime = Arc::new(test_runtime());
                register_failure_runner(&runtime).await;
                let revision = seed_failure_edit_revision(&runtime).await;
                let service = Service::new(build_test_router(
                    test_config(Some("secret")),
                    db,
                    runtime.clone(),
                ));
                let arguments = json!({"project":"agent:failure-runner:probe","dry_run":dry_run,"changes":[{
                    "kind":"edit","path":"probe.txt","expected_read_revision":revision as f64,
                    "edits":[{"kind":"replace_exact","old_text":"before","new_text":"after"}]
                }]});
                let params = if gateway {
                    adaptive_runtime_gateway_params("edit_project_files", arguments)
                } else {
                    json!({"name":"edit_project_files","arguments":arguments})
                };
                let (response, ()) = tokio::join!(
                    http_call(&service, params, client_meta(client, "fixture"), modern),
                    async {
                        let request = wait_for_failure_request(&runtime).await;
                        assert_eq!(request.kind, "file_apply_text_edits");
                        let output = json!({"dry_run":dry_run,"execution_state":"completed","applied_count":if dry_run{0}else{1},"planned_count":1,
                            "changed":!dry_run,"state_changed":!dry_run,"would_change":true,
                            "changed_paths":if dry_run {vec![]}else{vec!["probe.txt"]},
                            "files":[{"index":0,"kind":"edit","path":"probe.txt","to_path":null,
                                "old_sha256":"a".repeat(64),"new_sha256":if dry_run{"a".repeat(64)}else{"b".repeat(64)},
                                "changed":!dry_run,"would_change":true,
                                "edits":[{"index":0,"kind":"replace_exact","old_start_line":1,"old_end_line":1,"new_line_count":1}]}]
                        });
                        runtime
                            .runner_registry
                            .complete(RunnerResultRequest {
                                client_id: "failure-runner".into(),
                                runner_instance_id: "inst".into(),
                                request_id: request.request_id,
                                exit_code: Some(0),
                                stdout: Some(output.to_string()),
                                stderr: Some(String::new()),
                                stdout_truncated: false,
                                stderr_truncated: false,
                                duration_ms: Some(1),
                                error: None,
                            })
                            .await
                            .unwrap();
                    }
                );
                assert_eq!(response.0, StatusCode::OK, "{}", response.1);
                let result = &response.1["result"];
                assert_eq!(
                    result["isError"], false,
                    "modern={modern} gateway={gateway} dry_run={dry_run}: {result}"
                );
                assert_eq!(result["structuredContent"]["success"], true, "{result}");
                let output = &result["structuredContent"]["output"];
                assert_eq!(output["changed"], !dry_run);
                if dry_run {
                    assert!(
                        output["files"][0]["read_revision"].is_null(),
                        "dry-run does not mint future revision authority: {output}"
                    );
                    assert_eq!(output["state_changed"], false);
                    assert_eq!(output["would_change"], true);
                } else {
                    assert!(output["files"][0]["read_revision"].is_u64(), "{output}");
                    assert!(
                        output.get("state_changed").is_none(),
                        "success consumers use changed, not a duplicate effect field"
                    );
                }
                assert!(
                    runtime
                        .runner_registry
                        .poll(RunnerPollRequest {
                            client_id: "failure-runner".into(),
                            runner_instance_id: "inst".into()
                        })
                        .await
                        .unwrap()
                        .is_none(),
                    "adapter must never replay an edit"
                );
            }
        }
    }
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn http_json_text_compat_is_selected_per_call_after_initialize() {
    let _env = crate::auth::AuthEnvGuard::new();
    _env.enable_direct_shared_key();
    _env.disable_open_anonymous();
    for force_json in [false, true] {
        let (_tmp, db) = test_db();
        let runtime = test_runtime_with_mcp_snapshot(true, true, force_json)
            .with_communication_database(db.clone());
        let service = Service::new(build_test_router(
            test_config(Some("secret")),
            db,
            Arc::new(runtime),
        ));
        let (status, init) = http_request(
            &service,
            "initialize",
            json!({
                "protocolVersion": "2025-06-18", "capabilities": {},
                "clientInfo": {"name":"Gemini","version":"1.0.0"}
            }),
            Some("Google"),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");

        // Interleave callers on one Server using the same credential.
        for (params, tool) in [
            (
                json!({"name":"get_runtime_status","arguments":{}}),
                "get_runtime_status",
            ),
            (
                adaptive_runtime_gateway_params(
                    "search_webcodex_resources",
                    json!({"kind":"project"}),
                ),
                "search_webcodex_resources",
            ),
        ] {
            for ua in [
                Some("openai-mcp/1.0.0"),
                Some("Google"),
                None,
                Some("other-client"),
                Some("Google"),
            ] {
                let (status, body) =
                    http_request(&service, "tools/call", params.clone(), ua, false).await;
                assert_eq!(status, StatusCode::OK, "{body}");
                assert_success_text(&body["result"], force_json || ua == Some("Google"));
                crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
                    &body["result"]["structuredContent"],
                    &crate::tool_runtime::registry::output_schema_for_tool(tool),
                )
                .unwrap();
            }
        }

        let params = adaptive_runtime_gateway_params(
            "read_workspace_changes",
            json!({"project":"agent:missing:missing"}),
        );
        let mut expected = None;
        for (ua, openai_meta) in [
            ("Google", false),
            ("openai-mcp/1.0.0", true),
            ("openai-mcp/1.0.0", false),
        ] {
            let mut params = params.clone();
            if openai_meta {
                params["_meta"] = client_meta("openai-mcp", "1.0.0");
            }
            let (status, body) =
                http_request(&service, "tools/call", params, Some(ua), false).await;
            assert_eq!(status, StatusCode::OK, "{body}");
            assert_eq!(
                assert_failure(&body, !openai_meta)["error_kind"],
                "unknown_project"
            );
            let result = &body["result"];
            let structured = &result["structuredContent"];
            let text = result["content"][0]["text"].as_str().unwrap();
            if force_json || ua == "Google" {
                assert_eq!(serde_json::from_str::<Value>(text).unwrap(), *structured);
            } else {
                assert_eq!(text, structured["error"].as_str().unwrap());
            }
            if let Some(expected) = &expected {
                assert_eq!(structured, expected);
            } else {
                expected = Some(structured.clone());
            }
        }
    }
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn http_json_text_compat_preserves_workbench_tool_and_resource_framing() {
    let _env = crate::auth::AuthEnvGuard::new();
    _env.enable_direct_shared_key();
    _env.disable_open_anonymous();
    let (_tmp, db) = test_db();
    let runtime =
        test_runtime_with_mcp_snapshot(true, true, false).with_communication_database(db.clone());
    let service = Service::new(build_test_router(
        test_config(Some("secret")),
        db,
        Arc::new(runtime),
    ));
    for ua in ["Google", "openai-mcp/1.0.0"] {
        // Workbench needs the existing text echo even on an OpenAI request.
        let (status, body) = http_request(
            &service,
            "tools/call",
            json!({"name":"search_webcodex_resources","arguments":{"kind":"project"}}),
            Some(ua),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_success_text(&body["result"], true);

        let uri = MCP_WORKBENCH_UI_RESOURCE_URI;
        let (status, body) = http_request(
            &service,
            "resources/read",
            json!({"uri":uri}),
            Some(ua),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let resource = &body["result"]["contents"][0];
        assert_eq!(resource["uri"], uri);
        assert_eq!(resource["mimeType"], "text/html;profile=mcp-app");
        assert!(resource["text"]
            .as_str()
            .unwrap()
            .contains("ui/update-model-context"));
        assert!(body["result"].get("structuredContent").is_none());
    }
}

use super::*;

// This is the reviewed public operation set, not a second authorization registry.
// Expected scopes deliberately remain explicit so exposure changes cannot silently
// weaken the canonical authority declarations.
const HOST_REVIEW_TOOLS: &[(&str, &[&str])] = &[
    (
        "create_agent_identity",
        &["communication:read", "communication:manage"],
    ),
    (
        "rotate_agent_continuation_endpoint",
        &["communication:read", "communication:manage"],
    ),
    ("list_agent_identities", &["communication:read"]),
    (
        "prepare_goal_workflow",
        &[
            "communication:read",
            "communication:manage",
            "session:collaborate",
        ],
    ),
    ("get_goal", &["communication:read"]),
    (
        "checkpoint_goal",
        &["communication:read", "communication:manage"],
    ),
    (
        "update_goal",
        &["communication:read", "communication:manage"],
    ),
    (
        "bootstrap_agent_conversation",
        &["communication:read", "communication:manage"],
    ),
    (
        "consume_agent_wake",
        &["communication:read", "communication:manage"],
    ),
];

fn review_auth(user: &str) -> crate::auth::AuthContext {
    let mut auth = crate::auth::AuthContext::new(crate::auth::AuthKind::OAuth2Token);
    auth.user_id = Some(user.to_string());
    auth.username = Some(user.to_string());
    auth.token_kind = Some("oauth2".to_string());
    auth.scopes = [
        "runtime:read",
        "communication:read",
        "communication:manage",
        "session:collaborate",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    auth
}

fn review_params(params: Value, modern: bool) -> Value {
    if modern {
        mcp_2026_params(params)
    } else {
        params
    }
}

fn create_arguments(key: &str) -> Value {
    json!({"handle": "review-agent", "display_name": "Review Agent", "idempotency_key": key})
}

fn review_runtime(compact: bool, apps: bool) -> (tempfile::TempDir, ToolRuntime) {
    let temp = tempfile::tempdir().unwrap();
    let db =
        std::sync::Arc::new(crate::Database::open(&temp.path().join("host-review.db")).unwrap());
    let runtime = with_workflow_tools(test_runtime_with_mcp_settings(compact, apps))
        .with_communication_database(db)
        .with_permission_evaluator(crate::tool_runtime::PermissionEvaluator::with_mode(
            crate::tool_runtime::AuthorityMode::TrustedAgent,
        ));
    (temp, runtime)
}

fn metadata_free(mut value: Value) -> Value {
    fn strip(value: &mut Value) {
        match value {
            Value::Object(object) => {
                object.remove("description");
                object.remove("title");
                for child in object.values_mut() {
                    strip(child);
                }
            }
            Value::Array(array) => {
                for child in array {
                    strip(child);
                }
            }
            _ => {}
        }
    }
    strip(&mut value);
    value
}

fn agent_count(runtime: &ToolRuntime, auth: Option<&crate::auth::AuthContext>) -> u64 {
    let result = runtime.list_agent_identities(
        auth,
        webcodex_tool_contracts::ListAgentIdentitiesInput {
            agent_id: None,
            offset: None,
            limit: None,
        },
    );
    assert!(result.success, "{:?}", result.output);
    result.output["total_count"].as_u64().unwrap()
}

#[tokio::test]
async fn host_review_public_descriptors_and_manifest_routes_are_exact() {
    let specs = registered_tool_specs();
    let auth = review_auth("review-owner");
    for compact in [false, true] {
        for apps in [false, true] {
            let runtime = with_workflow_tools(test_runtime_with_mcp_settings(compact, apps));
            for modern in [false, true] {
                let McpOutcome::Ok(body) = handle_mcp_request(
                    &runtime,
                    rpc(
                        "tools/list",
                        Some(json!(1)),
                        review_params(json!({}), modern),
                    ),
                    Some(&auth),
                )
                .await
                else {
                    panic!("tools/list compact={compact} apps={apps} modern={modern}");
                };
                let tools = body["result"]["tools"].as_array().unwrap();
                for &(name, scopes) in HOST_REVIEW_TOOLS {
                    let matches = tools
                        .iter()
                        .filter(|tool| tool["name"] == name)
                        .collect::<Vec<_>>();
                    assert_eq!(
                        matches.len(),
                        1,
                        "{name}: compact={compact} apps={apps} modern={modern}"
                    );
                    let descriptor = matches[0];
                    assert_ne!(
                        descriptor.pointer("/_meta/ui/visibility"),
                        Some(&json!(["app"])),
                        "{name} must be model-callable, not merely App-visible"
                    );
                    let canonical = specs.iter().find(|spec| spec.name == name).unwrap();
                    if modern && !compact {
                        let control = &descriptor["inputSchema"]["properties"]["_wc"]["properties"]
                            ["control"]["properties"];
                        let before = control["before"]["properties"].as_object().unwrap();
                        let after = control["after_success"]["properties"].as_object().unwrap();
                        assert!(!before.contains_key("goal_progress"), "{name}");
                        assert!(!before.contains_key("wake_consume"), "{name}");
                        assert!(!after.contains_key("goal_completion"), "{name}");
                        assert!(before.contains_key("attempt_heartbeat"), "{name}");
                        assert!(after.contains_key("todo_completion"), "{name}");
                    }
                    assert!(!descriptor["description"].as_str().unwrap().is_empty());
                    assert_eq!(descriptor["annotations"], canonical.annotations, "{name}");
                    assert_eq!(
                        descriptor["inputSchema"]["additionalProperties"], false,
                        "{name}"
                    );
                    assert_eq!(
                        descriptor["inputSchema"]["required"], canonical.input_schema["required"],
                        "{name}"
                    );
                    // Compare all business-field validation, allowing only discovery
                    // prose compaction and the protocol-owned _wc envelope.
                    for (field, schema) in canonical.input_schema["properties"].as_object().unwrap()
                    {
                        assert_eq!(
                            metadata_free(descriptor["inputSchema"]["properties"][field].clone()),
                            metadata_free(schema.clone()),
                            "{name}.{field}"
                        );
                    }
                    assert_eq!(descriptor.get("outputSchema").is_some(), !compact, "{name}");
                    if !compact {
                        // MCP may append protocol-owned continuation/context
                        // schemas; the canonical result envelope remains intact.
                        assert_eq!(
                            descriptor["outputSchema"]["type"], canonical.output_schema["type"],
                            "{name}"
                        );
                        assert_eq!(
                            descriptor["outputSchema"]["properties"]["success"],
                            canonical.output_schema["properties"]["success"],
                            "{name}"
                        );
                    }
                    assert_eq!(
                        descriptor["securitySchemes"],
                        json!([{ "type": "oauth2", "scopes": scopes }]),
                        "{name}"
                    );
                    let definition =
                        webcodex_tool_contracts::tool_definition::lookup_tool_definition(name)
                            .unwrap();
                    assert_eq!(
                        definition.metadata.authority,
                        webcodex_core::authority::ToolAuthorityPolicy::RequireAll(scopes),
                        "{name}"
                    );

                    let McpOutcome::Ok(manifest) = handle_mcp_request(
                        &runtime,
                        rpc(
                            "tools/call",
                            Some(json!(2)),
                            review_params(
                                json!({
                                    "name": "read_tool_manifest", "arguments": {"tool_name": name}
                                }),
                                modern,
                            ),
                        ),
                        Some(&auth),
                    )
                    .await
                    else {
                        panic!("manifest {name}");
                    };
                    let output = &manifest["result"]["structuredContent"]["output"];
                    assert_eq!(output["route"]["primary"]["mode"], "direct", "{manifest}");
                    assert_eq!(output["route"]["primary"]["tool"], name, "{manifest}");
                    assert!(output["route"]["fallback"].is_null(), "{manifest}");
                    let business_schema = if modern {
                        output["input_schema"].clone()
                    } else {
                        super::cli::business_schema_with_collaboration_checked(
                            &output["input_schema"],
                        )
                    };
                    assert_eq!(business_schema, canonical.input_schema, "{name}");
                    assert_eq!(output["annotations"], canonical.annotations, "{name}");
                }
            }
        }
    }
}

#[tokio::test]
async fn host_review_oauth_missing_each_scope_hides_descriptor_and_challenges_direct_call() {
    for apps in [false, true] {
        let runtime = with_workflow_tools(test_runtime_with_mcp_settings(true, apps));
        for modern in [false, true] {
            for &(name, scopes) in HOST_REVIEW_TOOLS {
                for &missing in scopes {
                    let mut auth = review_auth("review-owner");
                    auth.scopes.retain(|scope| scope != missing);
                    let McpOutcome::Ok(body) = handle_mcp_request(
                        &runtime,
                        rpc(
                            "tools/list",
                            Some(json!(3)),
                            review_params(json!({}), modern),
                        ),
                        Some(&auth),
                    )
                    .await
                    else {
                        panic!("scope-filtered tools/list {name}");
                    };
                    assert!(
                        !body["result"]["tools"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|tool| tool["name"] == name),
                        "{name} missing {missing}"
                    );
                    let denied = handle_mcp_request(&runtime,
                        rpc("tools/call", Some(json!(4)), review_params(json!({
                            "name": name, "arguments": if name == "create_agent_identity" { create_arguments("scope-denied") } else { json!({}) }
                        }), modern)), Some(&auth)).await;
                    let McpOutcome::Forbidden {
                        body,
                        required_scope,
                    } = denied
                    else {
                        panic!("direct scope denial {name} missing {missing}: {denied:?}");
                    };
                    assert_eq!(required_scope, Some(missing), "{name}: {body}");
                    assert_eq!(body["error"], "insufficient_scope", "{name}: {body}");
                    let challenges = body["_meta"]["mcp/www_authenticate"].as_array().unwrap();
                    assert_eq!(challenges.len(), 1);
                    assert_eq!(
                        challenges[0],
                        crate::auth::oauth_insufficient_scope_challenge(Some(missing))
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn host_review_gateway_rejects_before_target_execution_in_every_protocol_and_app_mode() {
    for apps in [false, true] {
        let (_temp, runtime) = review_runtime(true, apps);
        let auth = review_auth("review-owner");
        assert_eq!(agent_count(&runtime, Some(&auth)), 0);
        for modern in [false, true] {
            for &(name, _) in HOST_REVIEW_TOOLS {
                let arguments = if name == "create_agent_identity" {
                    create_arguments("gateway-must-not-create")
                } else {
                    json!({})
                };
                let outcome = handle_mcp_request(
                    &runtime,
                    rpc(
                        "tools/call",
                        Some(json!(5)),
                        review_params(adaptive_runtime_gateway_params(name, arguments), modern),
                    ),
                    Some(&auth),
                )
                .await;
                let McpOutcome::BadRequest(body) = outcome else {
                    panic!("gateway must reject reviewed operation {name}: {outcome:?}");
                };
                assert_eq!(body["error"]["code"], -32602, "{body}");
                assert!(
                    body["error"]["message"]
                        .as_str()
                        .unwrap()
                        .contains("direct_host_review_required"),
                    "{body}"
                );
                assert_eq!(
                    agent_count(&runtime, Some(&auth)),
                    0,
                    "gateway executed {name}"
                );
            }
        }
    }
}

#[tokio::test]
async fn host_review_rejected_gateway_does_not_apply_control_sidecars() {
    let (_temp, runtime) = review_runtime(true, true);
    let auth = review_auth("review-owner");
    let created = runtime.create_goal_with_plan(
        Some(&auth),
        crate::db::NewGoal {
            title: "Rejected route".into(),
            objective: "No sidecar effects before route admission".into(),
            controller_agent_id: None,
            completion_conditions: vec![],
            steps: vec![crate::db::NewGoalStep {
                id: "inspect".into(),
                title: "Inspect".into(),
            }],
            idempotency_key: "route-admission-goal".into(),
        },
    );
    assert!(created.success);
    let goal_id = created.output["goal"]["summary"]["goal_id"]
        .as_str()
        .unwrap();
    let outcome = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(12)),
            mcp_2026_params(json!({
                "name": "call_runtime_tool",
                "arguments": {
                    "tool": "get_goal", "arguments": {"goal_id": goal_id},
                    "_wc": {"control": {"before": {"goal_progress": {
                        "goal_id": goal_id, "expected_revision": 1,
                        "completed_step_ids": ["inspect"], "summary": "must not run",
                        "idempotency_key": "denied-sidecar"
                    }}}}
                }
            })),
        ),
        Some(&auth),
    )
    .await;
    let McpOutcome::BadRequest(body) = outcome else {
        panic!("review-required target must fail before sidecar effects: {outcome:?}");
    };
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .starts_with("direct_host_review_required:"));
    let goal = runtime.get_goal(Some(&auth), goal_id.to_string());
    assert!(goal.success);
    assert_eq!(goal.output["goal"]["summary"]["revision"], 1);
}

#[tokio::test]
async fn host_review_sidecar_forms_are_rejected_on_direct_and_gateway_calls() {
    let goal = "wc_goal_AAAAAAAAAAAAAAAA";
    let cases = [
        (
            "checkpoint_goal",
            json!({"before": {"goal_progress": {
                "goal_id": goal, "expected_revision": 1, "summary": "reviewed progress",
                "completed_step_ids": ["inspect"], "idempotency_key": "sidecar-progress"
            }}}),
        ),
        (
            "consume_agent_wake",
            json!({"before": {"wake_consume": {
                "agent_id": "wc_dagent_AAAAAAAAAAAAAAAA",
                "endpoint_id": "wc_endpoint_AAAAAAAAAAAAAAAA",
                "expected_controller_generation": 1,
                "wake_id": "wc_wake_AAAAAAAAAAAAAAAA",
                "consume_token": "wc_wake_consume_test_fixture_only"
            }}}),
        ),
        (
            "update_goal",
            json!({"after_success": {"goal_completion": {
                "goal_id": goal, "expected_revision": 1,
                "idempotency_key": "sidecar-complete"
            }}}),
        ),
    ];
    for apps in [false, true] {
        let (_temp, runtime) = review_runtime(true, apps);
        let auth = review_auth("review-owner");
        for (canonical, control) in &cases {
            for gateway in [false, true] {
                let mut params = if gateway {
                    adaptive_runtime_gateway_params("list_projects", json!({}))
                } else {
                    json!({"name": "get_runtime_status", "arguments": {}})
                };
                params["arguments"]["_wc"] = json!({"control": control});
                let outcome = handle_mcp_request(
                    &runtime,
                    rpc("tools/call", Some(json!(13)), mcp_2026_params(params)),
                    Some(&auth),
                )
                .await;
                let McpOutcome::BadRequest(body) = outcome else {
                    panic!("{canonical} sidecar must reject before any dispatch: {outcome:?}");
                };
                assert_eq!(body["error"]["code"], -32602);
                assert!(body["error"]["message"]
                    .as_str()
                    .unwrap()
                    .starts_with(&format!("direct_host_review_required: '{canonical}'")));
                assert_eq!(agent_count(&runtime, Some(&auth)), 0);
            }
        }
    }
}

#[tokio::test]
async fn host_review_retained_session_message_sidecar_executes_and_replays_through_mcp() {
    for apps in [false, true] {
        for gateway in [false, true] {
            let (_temp, runtime) = review_runtime(true, apps);
            let auth = review_auth("review-owner");
            let session = start_authorized_test_session(
                &runtime,
                &auth,
                crate::tool_runtime::SessionMode::Normal,
            );
            let mut params = if gateway {
                adaptive_runtime_gateway_params("get_runtime_status", json!({}))
            } else {
                json!({"name": "get_runtime_status", "arguments": {}})
            };
            params["arguments"]["_wc"] = json!({"control": {"communication": {
                "before": [{"session_message": {
                    "session_id": session.session_id,
                    "kind": "progress",
                    "message": "Retained communication passed through the MCP adapter",
                    "delivery_key": "retained-mcp-communication"
                }}]
            }}});

            // The same envelope is unavailable on legacy MCP, before any
            // sidecar effect. Its successful route is the admitted 2026 adapter.
            let legacy = handle_mcp_request(
                &runtime,
                rpc("tools/call", Some(json!(14)), params.clone()),
                Some(&auth),
            )
            .await;
            let McpOutcome::BadRequest(body) = legacy else {
                panic!("legacy MCP must reject the invocation envelope: {legacy:?}");
            };
            assert_eq!(body["error"]["code"], -32602);
            assert!(body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("unsupported _wc field 'control'"));
            assert!(runtime
                .sessions
                .list_messages(&session.session_id, Default::default())
                .unwrap()
                .is_empty());

            let mut message_id = None;
            for replay in [false, true] {
                let outcome = handle_mcp_request(
                    &runtime,
                    rpc(
                        "tools/call",
                        Some(json!(15)),
                        mcp_2026_params(params.clone()),
                    ),
                    Some(&auth),
                )
                .await;
                let McpOutcome::Ok(body) = outcome else {
                    panic!("retained sidecar apps={apps} gateway={gateway}: {outcome:?}");
                };
                assert_eq!(body["result"]["isError"], false, "{body}");
                let control = &body["result"]["structuredContent"]["output"]["control"];
                assert_eq!(control["main"]["success"], true, "{body}");
                let receipt = &control["communication"]["before"][0];
                assert_eq!(receipt["kind"], "session_message", "{body}");
                assert_eq!(receipt["success"], true, "{body}");
                assert_eq!(receipt["replayed"], replay, "{body}");
                assert_eq!(receipt["state_changed"], !replay, "{body}");
                if let Some(expected) = &message_id {
                    assert_eq!(&receipt["message_id"], expected, "{body}");
                } else {
                    assert!(receipt["message_id"]
                        .as_str()
                        .unwrap()
                        .starts_with("wc_msg_"));
                    message_id = Some(receipt["message_id"].clone());
                }
                assert_eq!(
                    runtime
                        .sessions
                        .list_messages(&session.session_id, Default::default())
                        .unwrap()
                        .len(),
                    1,
                    "the replay must retain one actual Session message"
                );
            }
        }
    }
}

#[tokio::test]
async fn host_review_direct_creation_preserves_keyed_replay_and_conflict() {
    for apps in [false, true] {
        let (_temp, runtime) = review_runtime(true, apps);
        let auth = review_auth("review-owner");
        let mut agent_id = None;
        for (index, modern) in [false, true].into_iter().enumerate() {
            let McpOutcome::Ok(body) = handle_mcp_request(&runtime,
                rpc("tools/call", Some(json!(6)), review_params(json!({
                    "name": "create_agent_identity", "arguments": create_arguments("direct-create")
                }), modern)), Some(&auth)).await
            else { panic!("legitimate direct create"); };
            assert_eq!(body["result"]["isError"], false, "{body}");
            let output = &body["result"]["structuredContent"]["output"];
            assert_eq!(output["created"], index == 0, "{body}");
            assert_eq!(output["replayed"], index != 0, "{body}");
            assert_eq!(output["state_changed"], index == 0, "{body}");
            if let Some(expected) = &agent_id {
                assert_eq!(&output["agent"]["agent_id"], expected);
            } else {
                agent_id = Some(output["agent"]["agent_id"].clone());
            }
            assert_eq!(agent_count(&runtime, Some(&auth)), 1);
        }
        let mut changed = create_arguments("direct-create");
        changed["display_name"] = json!("Changed request");
        let McpOutcome::Ok(body) = handle_mcp_request(
            &runtime,
            rpc(
                "tools/call",
                Some(json!(7)),
                mcp_2026_params(json!({
                    "name": "create_agent_identity", "arguments": changed
                })),
            ),
            Some(&auth),
        )
        .await
        else {
            panic!("canonical replay conflict receipt");
        };
        assert_eq!(body["result"]["isError"], true, "{body}");
        assert_eq!(
            body["result"]["structuredContent"]["output"]["error_kind"],
            "communication_idempotency_conflict",
            "{body}"
        );
        assert_eq!(agent_count(&runtime, Some(&auth)), 1);
    }
}

#[tokio::test]
async fn host_review_direct_route_keeps_permission_input_and_owner_guards() {
    let (_temp, runtime) = review_runtime(true, true);
    let auth = review_auth("review-owner");
    let mut invalid = create_arguments("invalid-input");
    invalid["unknown_authority"] = json!(true);
    let rejected = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(8)),
            mcp_2026_params(json!({
                "name": "create_agent_identity", "arguments": invalid
            })),
        ),
        Some(&auth),
    )
    .await;
    assert!(
        matches!(rejected, McpOutcome::BadRequest(_)),
        "{rejected:?}"
    );
    assert_eq!(agent_count(&runtime, Some(&auth)), 0);

    let restricted = runtime.clone().with_permission_evaluator(
        crate::tool_runtime::PermissionEvaluator::with_mode(
            crate::tool_runtime::AuthorityMode::Restricted,
        ),
    );
    let McpOutcome::Ok(body) = handle_mcp_request(
        &restricted,
        rpc(
            "tools/call",
            Some(json!(9)),
            mcp_2026_params(json!({
                "name": "create_agent_identity", "arguments": create_arguments("restricted")
            })),
        ),
        Some(&auth),
    )
    .await
    else {
        panic!("canonical permission-denied receipt");
    };
    assert_eq!(body["result"]["isError"], true, "{body}");
    assert_eq!(
        body["result"]["structuredContent"]["output"]["error_kind"], "permission_denied",
        "{body}"
    );
    assert_eq!(agent_count(&runtime, Some(&auth)), 0);

    let McpOutcome::Ok(created) = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(10)),
            mcp_2026_params(json!({
                "name": "create_agent_identity", "arguments": create_arguments("owned")
            })),
        ),
        Some(&auth),
    )
    .await
    else {
        panic!("owned Agent creation");
    };
    let agent_id = &created["result"]["structuredContent"]["output"]["agent"]["agent_id"];
    let foreign = review_auth("different-owner");
    let McpOutcome::Ok(denied) = handle_mcp_request(
        &runtime,
        rpc(
            "tools/call",
            Some(json!(11)),
            mcp_2026_params(json!({
                "name": "rotate_agent_continuation_endpoint", "arguments": {
                    "agent_id": agent_id, "host": "ChatGPT", "idempotency_key": "foreign-endpoint"
                }
            })),
        ),
        Some(&foreign),
    )
    .await
    else {
        panic!("canonical ownership-denied receipt");
    };
    assert_eq!(denied["result"]["isError"], true, "{denied}");
    assert_eq!(
        denied["result"]["structuredContent"]["output"]["error_kind"], "agent_not_found",
        "{denied}"
    );
    assert_eq!(agent_count(&runtime, Some(&foreign)), 0);
    assert_eq!(agent_count(&runtime, Some(&auth)), 1);
}

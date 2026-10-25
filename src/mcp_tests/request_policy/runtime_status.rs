use super::*;

#[tokio::test]
async fn request_policy_runtime_status_reports_effective_profile_and_request_source() {
    let (_tmp, db) = test_db();
    let runtime = Arc::new(test_runtime().with_mcp_host_policy(deployment()));
    let service = Service::new(build_test_router(
        test_config(Some("secret")),
        db,
        runtime.clone(),
    ));
    for compact in [true, false] {
        for gateway in [false, true] {
            let params = if gateway {
                adaptive_runtime_gateway_params("get_runtime_status", json!({"compact": compact}))
            } else {
                json!({"name": "get_runtime_status", "arguments": {"compact": compact}})
            };
            let (direct, same_as_deployment, omitted) = tokio::join!(
                post(
                    &service,
                    "tools/call",
                    params.clone(),
                    Some("direct"),
                    Some("9")
                ),
                post(
                    &service,
                    "tools/call",
                    params.clone(),
                    Some("host_code_mode"),
                    None
                ),
                post(&service, "tools/call", params.clone(), None, None),
            );
            for (response, effective, source) in [
                (direct, "direct", "request_header"),
                (same_as_deployment, "host_code_mode", "request_header"),
                (omitted, "host_code_mode", "deployment"),
            ] {
                assert_eq!(response.0, StatusCode::OK, "{}", response.1);
                let result = &response.1["result"]["structuredContent"];
                assert_eq!(result["success"], true, "{result}");
                let output = &result["output"];
                assert_eq!(
                    output["mcp_host"],
                    json!({
                        "profile": effective,
                        "deployment_profile": "host_code_mode",
                        "profile_source": source,
                    })
                );
                if !compact {
                    assert_eq!(
                        output["effective_config"]["mcp_host"]["profile"],
                        "host_code_mode"
                    );
                    assert_eq!(
                        output["effective_config"]["mcp_host"]["host_budget_secs"],
                        55
                    );
                    assert_eq!(
                        output["effective_config"]["mcp_host"]["max_sync_wait_secs"],
                        5
                    );
                }
                let schema =
                    crate::tool_runtime::registry::output_schema_for_tool("get_runtime_status");
                crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
                    result, &schema,
                )
                .unwrap();
                let mut invalid_source = result.clone();
                invalid_source["output"]["mcp_host"]["profile_source"] = json!("previous_host");
                assert!(
                    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
                        &invalid_source,
                        &schema,
                    )
                    .is_err()
                );
                if compact && !gateway {
                    println!(
                        "RUNTIME_STATUS_PROFILE_BYTES effective={effective} source={source} bytes={}",
                        serde_json::to_vec(&output["mcp_host"]).unwrap().len(),
                    );
                }
            }
            let budget_only = post(&service, "tools/call", params.clone(), None, Some("9")).await;
            assert_eq!(
                budget_only.1["result"]["structuredContent"]["output"]["mcp_host"]
                    ["profile_source"],
                "deployment",
            );
            let later = post(&service, "tools/call", params, None, None).await;
            assert_eq!(
                later.1["result"]["structuredContent"]["output"]["mcp_host"],
                json!({
                    "profile": "host_code_mode", "deployment_profile": "host_code_mode",
                    "profile_source": "deployment",
                }),
            );
        }
    }
    assert_eq!(runtime.mcp_host_policy, deployment());
    assert_eq!(runtime.deployment_mcp_host_policy, deployment());
}

use super::*;
use webcodex_tool_contracts::test_support::validate_schema_instance;

fn assert_computer_key_manifest_contract(schema: &Value) {
    for modifiers in [
        None,
        Some(Value::Null),
        Some(json!([])),
        Some(json!(["shift"])),
        Some(json!(["shift", "control", "option", "command"])),
    ] {
        let mut input =
            json!({"action":"key","client_id":"computer","surface_id":"surface","key":"tab"});
        if let Some(modifiers) = modifiers {
            input["modifiers"] = modifiers;
        }
        validate_schema_instance(&input, schema).unwrap_or_else(|error| panic!("{input}: {error}"));
        assert!(crate::tool_runtime::ToolCall::from_tool_name("control_computer", input).is_ok());
    }
    for (key, modifiers) in [
        ("p", json!([])),
        ("Ctrl+P", json!([])),
        ("Enter", json!([])),
        ("tab", json!(["ctrl"])),
        ("tab", json!(["alt"])),
        ("tab", json!(["Shift"])),
        ("tab", json!(["shift", "shift"])),
        (
            "tab",
            json!(["shift", "control", "option", "command", "shift"]),
        ),
        ("tab", json!("shift")),
        ("tab", json!(true)),
        ("tab", json!(0)),
        ("tab", json!({})),
        ("tab", json!([null])),
        ("tab", json!([1])),
    ] {
        let input = json!({"action":"key","client_id":"computer","surface_id":"surface","key":key,"modifiers":modifiers});
        assert!(validate_schema_instance(&input, schema).is_err(), "{input}");
    }
}

#[tokio::test]
async fn mcp_computer_key_contract_matches_direct_and_gateway_manifests() {
    let canonical = webcodex_tool_contracts::input_schema_for_tool("control_computer");
    for compact in [false, true] {
        let runtime = test_runtime_with_mcp_settings(compact, false);
        for gateway in [false, true] {
            let arguments = json!({"tool_name":"control_computer"});
            let params = if gateway {
                adaptive_runtime_gateway_params("read_tool_manifest", arguments)
            } else {
                json!({"name":"read_tool_manifest","arguments":arguments})
            };
            let McpOutcome::Ok(body) = handle_mcp_request(
                &runtime,
                rpc("tools/call", Some(json!(1)), mcp_2026_params(params)),
                None,
            )
            .await
            else {
                panic!("Computer manifest: compact={compact}, gateway={gateway}");
            };
            assert_eq!(body["result"]["isError"], false, "{body}");
            let output = &body["result"]["structuredContent"]["output"];
            assert_eq!(output["route"]["primary"]["mode"], "gateway");
            assert_eq!(output["route"]["primary"]["tool"], "call_runtime_tool");
            assert_eq!(output["route"]["primary"]["target"], "control_computer");
            assert_eq!(output["input_schema"], canonical);
            assert_computer_key_manifest_contract(&output["input_schema"]);
        }
    }
}

#[tokio::test]
async fn mcp_computer_key_contract_keeps_gateway_only_discovery() {
    for compact in [false, true] {
        let runtime = test_runtime_with_mcp_settings(compact, false);
        let McpOutcome::Ok(body) = handle_mcp_request(
            &runtime,
            rpc("tools/list", Some(json!(1)), mcp_2026_params(json!({}))),
            None,
        )
        .await
        else {
            panic!("Computer discovery: compact={compact}");
        };
        let tools = body["result"]["tools"].as_array().unwrap();
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "read_tool_manifest"));
        assert!(tools.iter().any(|tool| tool["name"] == "call_runtime_tool"));
        assert!(!tools.iter().any(|tool| tool["name"] == "control_computer"));
    }
}

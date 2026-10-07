use crate::tool_call::{BrowserActToolCall, BrowserObserveToolCall};
use serde_json::json;

#[test]
fn existing_launch_wire_shape_and_optional_mode_roundtrip_are_compatible() {
    let original = json!({"action":"launch","client_id":"mini"});
    let launch: BrowserActToolCall = serde_json::from_value(original.clone()).unwrap();
    assert_eq!(serde_json::to_value(launch).unwrap(), original);
    let managed = json!({"action":"launch","client_id":"mini","mode":"managed","profile":"login"});
    let launch: BrowserActToolCall = serde_json::from_value(managed.clone()).unwrap();
    assert_eq!(serde_json::to_value(launch).unwrap(), managed);
    for invalid in [
        json!({"action":"launch","client_id":"mini","mode":"external"}),
        json!({"action":"attach","client_id":"mini","attachment_id":"attachment_fixture","endpoint":"ws://127.0.0.1:9222"}),
        json!({"action":"launch","client_id":"mini","profile_path":"/user/Chrome/Default"}),
        json!({"action":"attach","client_id":"mini","attachment_id":"attachment_fixture","pid":123}),
    ] {
        assert!(serde_json::from_value::<BrowserActToolCall>(invalid).is_err());
    }
    assert!(serde_json::from_value::<BrowserObserveToolCall>(
        json!({"action":"surface","client_id":"mini","browser_id":"browser_fixture","pid":123})
    )
    .is_err());
}

#[test]
fn new_browser_capabilities_are_never_implied_by_an_old_runner() {
    let capabilities: webcodex_core::runner_protocol::RunnerCapabilities =
        serde_json::from_value(json!({
            "browser_launch":true, "browser_observe":true, "browser_control":true
        }))
        .unwrap();
    assert!(capabilities.browser_launch);
    assert!(!capabilities.browser_managed_profile);
    assert!(!capabilities.browser_extension_bridge);
    assert!(!capabilities.browser_surface_handoff);
}

#[test]
fn continuity_outputs_are_closed_and_do_not_need_transport_identity() {
    let browser = json!({"success":true,"output":{
        "browser_id":"browser_abcdefghijklmnop","page_count":1,
        "ownership":"owned_managed_persistent","profile":"login",
        "execution_state":"completed","state_changed":true
    },"error":null});
    let schema = crate::output_schema_for_tool("control_browser");
    crate::test_support::validate_schema_instance(&browser, &schema).unwrap();
    let mut unsafe_output = browser;
    unsafe_output["output"]["debugger_endpoint"] = json!("ws://127.0.0.1:9222");
    assert!(crate::test_support::validate_schema_instance(&unsafe_output, &schema).is_err());
}

#[test]
fn semantic_query_schema_is_closed_and_preserves_snapshot_shape() {
    let value = json!({"action":"snapshot","client_id":"mini", "browser_id":"browser_abcdefghijklmnop", "page_id":"page_abcdefghijklmnop", "query":{"fields_only":true,"text":"School","group":"Education","section":"History"}});
    let schema = crate::input_schema_for_tool("observe_browser");
    crate::test_support::validate_schema_instance(&value, &schema).unwrap();
    let call: BrowserObserveToolCall = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(call).unwrap()["query"], value["query"]);
    let mut invalid = value;
    invalid["query"]["selector"] = json!("div");
    assert!(serde_json::from_value::<BrowserObserveToolCall>(invalid.clone()).is_err());
    assert!(crate::test_support::validate_schema_instance(&invalid, &schema).is_err());
}

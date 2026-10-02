use super::PluginToolCall;
use serde_json::json;

#[test]
fn project_bound_plugin_describe_accepts_only_an_exact_bounded_project() {
    let base =
        json!({"action":"describe", "runner":"runner", "plugin":"pi", "tool":"agent_environment"});
    let mut request = base.clone();
    request["project"] = json!("agent:runner:repo");
    assert!(serde_json::from_value::<PluginToolCall>(request)
        .unwrap()
        .validate()
        .is_ok());
    for invalid in ["".to_string(), "bad\nproject".to_string(), "x".repeat(513)] {
        let mut request = base.clone();
        request["project"] = json!(invalid);
        assert!(serde_json::from_value::<PluginToolCall>(request)
            .unwrap()
            .validate()
            .is_err());
    }
}

#[test]
fn plugin_call_cannot_retarget_a_described_binding_with_a_project_argument() {
    let call: PluginToolCall = serde_json::from_value(json!({
        "action":"call", "project":"agent:runner:other", "arguments":{}
    }))
    .unwrap();
    assert!(call.validate().unwrap_err().contains("action=describe"));
    for action in ["list", "reload", "check"] {
        let call: PluginToolCall = serde_json::from_value(json!({
            "action":action,"runner":"runner","project":"agent:runner:repo"
        }))
        .unwrap();
        assert!(call.validate().is_err());
    }
}

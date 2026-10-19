use super::{PluginToolCall, ToolCall};
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

#[test]
fn plugin_project_argument_recovery_distinguishes_call_from_invalid_describe() {
    let binding = format!("wc_pbind_{}", webcodex_core::compact::random_suffix::<16>());
    let call = json!({"action":"call", "binding":binding, "arguments":{}});
    assert!(ToolCall::from_tool_name("plugin_tool", call.clone()).is_ok());
    let mut retargeted = call;
    retargeted["project"] = json!("agent:private-runner:private-project");
    let error = ToolCall::from_tool_name("plugin_tool", retargeted).unwrap_err();
    for phrase in [
        "action=call accepts only binding and arguments",
        "same intended Project",
        "another Project",
        "action=describe",
        "unknown outcome",
    ] {
        assert!(error.contains(phrase), "missing {phrase}: {error}");
    }
    assert!(!error.contains(&binding));
    assert!(!error.contains("private-project"));
    for project in [" ".to_string(), "private\nproject".to_string(), "x".repeat(513)] {
        let error = ToolCall::from_tool_name("plugin_tool", json!({
            "action":"describe", "runner":"runner", "plugin":"pi",
            "tool":"agent_environment", "project":project
        })).unwrap_err();
        assert!(error.contains("project on action=describe must be an exact bounded Project"), "{error}");
        assert!(!error.contains("action=call accepts"));
        if !project.trim().is_empty() {
            assert!(!error.contains(&project));
        }
    }
}

#[test]
fn plugin_describe_shape_recovery_keeps_project_and_binding_roles_explicit() {
    let binding = format!("wc_pbind_{}", webcodex_core::compact::random_suffix::<16>());
    for extra in [json!({"binding":binding}), json!({"arguments":{}})] {
        let mut request = json!({"action":"describe", "runner":"runner", "plugin":"pi",
            "tool":"agent_environment", "project":"agent:runner:repo"});
        request.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        let error = ToolCall::from_tool_name("plugin_tool", request).unwrap_err();
        assert!(error.contains("projectBound"), "{error}");
        assert!(error.contains("project"), "{error}");
        assert!(error.contains("not binding or arguments"), "{error}");
    }
    let error = ToolCall::from_tool_name("plugin_tool", json!({
        "action":"describe", "runner":"runner", "plugin":"pi", "tool":"agent_environment",
        "project":"agent:runner:repo", "unexpected":true
    })).unwrap_err();
    assert!(error.contains("unknown field"));
}

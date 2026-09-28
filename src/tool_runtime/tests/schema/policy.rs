use super::*;

#[test]
fn runtime_policy_facades_match_all_tool_definitions() {
    use crate::tool_runtime::metadata::lookup_tool_metadata;
    use webcodex_tool_contracts::{runtime_tool_runner_capability, tool_definitions};

    for definition in tool_definitions() {
        let name = definition.name;
        assert_eq!(
            lookup_tool_metadata(name),
            Some(&definition.metadata()),
            "{name}"
        );
        assert_eq!(
            runtime_tool_runner_capability(name),
            definition.runner_capability,
            "{name}"
        );
        if definition.visibility.is_model_visible() {
            let call = ToolCall::from_tool_name(name, sample_tool_args(name))
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(
                required_runner_capability(&call),
                definition.runner_capability,
                "{name}"
            );
        }
    }
    let unknown = "__unknown_contract_tool__";
    assert!(lookup_tool_metadata(unknown).is_none());
    // The capability facade requires a known dispatch identity; None means a
    // known tool needs no Runner capability, never acceptance of an unknown tool.
    assert!(std::panic::catch_unwind(|| runtime_tool_runner_capability(unknown)).is_err());
    assert!(ToolCall::from_tool_name(unknown, json!({})).is_err());
}

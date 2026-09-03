use super::*;

#[test]
fn tool_policy_helpers_match_tool_definitions_for_known_runtime_names() {
    for definition in tool_definitions() {
        assert_eq!(
            lookup_tool_definition(definition.name).map(|definition| definition.name),
            Some(definition.name)
        );
        assert_eq!(
            lookup_tool_metadata(definition.name).copied(),
            Some(definition.metadata())
        );
        assert_eq!(
            runtime_tool_metadata(definition.name),
            definition.metadata()
        );
        assert_eq!(
            runtime_tool_session_risk_class(definition.name),
            definition.session_risk_class()
        );
        assert_eq!(
            runtime_tool_is_read_like(definition.name),
            definition.is_read_like()
        );
        assert_eq!(
            runtime_tool_is_write_like(definition.name),
            definition.is_write_like()
        );
        assert_eq!(
            runtime_tool_is_shell_like(definition.name),
            definition.is_shell_like()
        );
        assert_eq!(runtime_tool_category(definition.name), definition.category);
        assert_eq!(
            runtime_tool_requires_permission(definition.name),
            definition.requires_permission()
        );
        assert_eq!(
            runtime_tool_approval_policy(definition.name),
            definition.metadata().approval
        );
        assert_eq!(
            runtime_tool_permission_risk(definition.name),
            definition.permission_risk()
        );
        assert_eq!(
            runtime_tool_runner_capability(definition.name),
            definition.runner_capability
        );
    }
}

#[test]
fn tool_metadata_has_no_non_runtime_entries() {
    for metadata in iter_tool_metadata() {
        assert!(is_known_tool_name(metadata.name), "{}", metadata.name);
    }
}

use super::*;

#[test]
fn edit_project_files_metadata_and_mcp_consistency() {
    use crate::tool_runtime::tool_definition::TOOL_CATEGORY_EDIT;

    // Known name + spec + metadata coverage. registered_tool_specs() backs
    // both the list_tools runtime tool and MCP tools/list (parity is enforced
    // by mcp_tools_list_parity_with_rest_tools_list), so checking specs covers
    // both surfaces.
    assert!(is_known_tool_name("edit_project_files"));
    let specs = registered_tool_specs();
    assert!(
        specs.iter().any(|s| s.name == "edit_project_files"),
        "edit_project_files must appear in registered tool specs (list_tools + MCP tools/list)"
    );
    for spec in &specs {
        assert!(
            is_known_tool_name(&spec.name),
            "{} must be recognized by ToolCall",
            spec.name
        );
    }
    assert!(
        specs.len()
            == crate::tool_runtime::tool_definition::model_visible_tool_definitions()
                .count(),
        "public specs must cover every model-visible runtime tool (ModelHidden tools are dispatched but have no spec)"
    );
    assert!(crate::tool_runtime::metadata::lookup_tool_metadata("edit_project_files").is_some());
    // The edit category includes the new tool.
    let cats = registered_tool_categories();
    let edit = cats[TOOL_CATEGORY_EDIT]
        .as_array()
        .expect("edit category present");
    assert!(edit.iter().any(|v| v == "edit_project_files"));
    #[cfg(feature = "legacy-gpt-actions")]
    {
        let openapi = crate::openapi::build_openapi_spec();
        let action = &openapi["paths"]["/api/actions/edit_project_files"]["post"];
        assert_eq!(action["operationId"], "edit_project_files");
        let action_schema = &action["requestBody"]["content"]["application/json"]["schema"];
        let canonical = specs
            .iter()
            .find(|spec| spec.name == "edit_project_files")
            .unwrap();
        assert_eq!(
            action_schema["required"],
            canonical.input_schema["required"]
        );
        assert_eq!(action_schema["additionalProperties"], false);
    }
}

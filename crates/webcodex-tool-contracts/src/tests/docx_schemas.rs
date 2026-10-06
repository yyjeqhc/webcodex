use super::*;

#[test]
fn docx_tools_reuse_the_generic_app_artifact_contract() {
    let present = lookup_tool_definition("present_docx").unwrap();
    assert!(present.visibility.is_model_visible());
    assert!(is_adaptive_runtime_direct_tool("present_docx"));
    assert!(lookup_tool_definition("read_docx_chunk").is_none());

    let generic_read = lookup_tool_definition("read_app_artifact_chunk").unwrap();
    assert!(!generic_read.visibility.is_model_visible());
    assert!(!registered_tool_names().contains(&"read_app_artifact_chunk".to_string()));

    let schema = input_schema_for_tool("present_docx");
    let mut required = schema["required"].as_array().unwrap().clone();
    required.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
    assert_eq!(required, vec![json!("path"), json!("project")]);
    assert_eq!(schema["additionalProperties"], false);

    let result = output_schema_for_tool("present_docx");
    assert!(result["properties"]["output"]["properties"]["docx_document"].is_object());

    let call = ToolCall::from_tool_name(
        "present_docx",
        json!({"project":"~p1","path":"report.docx"}),
    )
    .unwrap();
    assert_eq!(call.tool_name(), "present_docx");
    assert_eq!(call.project(), Some("~p1"));
    assert_eq!(call.session_id(), None);
}

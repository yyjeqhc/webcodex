use super::*;

#[test]
fn docx_tools_have_canonical_sparse_schemas_and_independent_visibility() {
    let present = lookup_tool_definition("present_docx").unwrap();
    assert!(present.visibility.is_model_visible());
    assert!(is_adaptive_runtime_direct_tool("present_docx"));
    let read = lookup_tool_definition("read_docx_chunk").unwrap();
    assert!(!read.visibility.is_model_visible());
    assert!(!registered_tool_names().contains(&"read_docx_chunk".to_string()));
    let schema = input_schema_for_tool("present_docx");
    let mut required = schema["required"].as_array().unwrap().clone();
    required.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
    assert_eq!(required, vec![json!("path"), json!("project")]);
    assert_eq!(schema["additionalProperties"], false);
    let schema = input_schema_for_tool("read_docx_chunk");
    assert_eq!(schema["properties"]["bytes"]["maximum"], 10485760);
    assert_eq!(docx_app_tool_specs()[0].name, "read_docx_chunk");
    let result = output_schema_for_tool("read_docx_chunk");
    assert!(
        result["properties"]["output"]["properties"]["docx_chunk"]["properties"]
            .get("content_base64")
            .is_none()
    );
    for (tool, arguments) in [
        (
            "present_docx",
            json!({"project":"~p1","path":"report.docx"}),
        ),
        (
            "read_docx_chunk",
            json!({"project":"~p1","path":"report.docx","sha256":"a".repeat(64),"bytes":512,"byte_offset":0}),
        ),
    ] {
        let call = ToolCall::from_tool_name(tool, arguments).unwrap();
        assert_eq!(call.tool_name(), tool);
        assert_eq!(call.project(), Some("~p1"));
        assert_eq!(call.session_id(), None);
    }
}

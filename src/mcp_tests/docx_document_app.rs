use super::*;

#[test]
fn docx_document_app_has_a_distinct_direct_binding_and_private_read_tool() {
    let payload =
        super::super::tools::mcp_tools_list_payload_with_features_for_auth(false, true, true, None);
    let tools = payload["tools"].as_array().unwrap();
    let present = tools
        .iter()
        .find(|tool| tool["name"] == "present_docx")
        .unwrap();
    assert_eq!(
        present["_meta"]["ui"]["resourceUri"],
        "ui://webcodex/docx/v1"
    );
    let read = tools
        .iter()
        .find(|tool| tool["name"] == "read_docx_chunk")
        .unwrap();
    assert_eq!(read["_meta"]["ui"]["visibility"], json!(["app"]));
    assert!(read["_meta"]["ui"].get("resourceUri").is_none());
    assert!(
        !super::super::tools::adaptive_runtime_gateway_target_admitted_for_test(
            "read_docx_chunk",
            true
        )
    );
    assert!(super::super::presentation::tool_requires_direct_app_presentation("present_docx"));
}

#[test]
fn docx_document_bytes_never_enter_public_mcp_content() {
    let encoded = "UEsDBFByaXZhdGVCeXRlcw==";
    let value = mcp_runtime_tool_result(
        "read_docx_chunk",
        false,
        ToolResult::ok(
            json!({"docx_chunk": {"project":"agent:docx:demo", "path":"report.docx", "content_base64": encoded}}),
        ),
    );
    assert_eq!(
        value["_meta"]["webcodex/docxChunk"]["content_base64"],
        encoded
    );
    assert!(!value["structuredContent"].to_string().contains(encoded));
    assert!(!value["content"].to_string().contains(encoded));
}

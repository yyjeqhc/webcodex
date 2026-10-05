use super::*;

#[test]
fn pdf_document_app_has_a_distinct_direct_binding_and_private_read_tool() {
    let payload =
        super::super::tools::mcp_tools_list_payload_with_features_for_auth(false, true, true, None);
    let tools = payload["tools"].as_array().unwrap();
    let present = tools
        .iter()
        .find(|tool| tool["name"] == "present_pdf")
        .unwrap();
    assert_eq!(
        present["_meta"]["ui"]["resourceUri"],
        "ui://webcodex/pdf/v3"
    );
    for name in ["read_app_artifact_chunk", "read_pdf_chunk"] {
        let read = tools.iter().find(|tool| tool["name"] == name).unwrap();
        assert_eq!(read["_meta"]["ui"]["visibility"], json!(["app"]));
        assert!(read["_meta"]["ui"].get("resourceUri").is_none());
        assert!(
            !super::super::tools::adaptive_runtime_gateway_target_admitted_for_test(name, true)
        );
    }
    assert!(super::super::presentation::tool_requires_direct_app_presentation("present_pdf"));
}

#[test]
fn pdf_document_bytes_never_enter_public_mcp_content() {
    let artifact_encoded = "QUJDRA==";
    let artifact = mcp_runtime_tool_result(
        "read_app_artifact_chunk",
        false,
        ToolResult::ok(
            json!({"artifact_chunk": {"project":"agent:pdf:demo", "path":"report.pdf", "content_base64": artifact_encoded}}),
        ),
    );
    assert_eq!(
        artifact["_meta"]["webcodex/artifactChunk"]["content_base64"],
        artifact_encoded
    );
    assert!(!artifact["structuredContent"]
        .to_string()
        .contains(artifact_encoded));
    assert!(!artifact["content"].to_string().contains(artifact_encoded));

    let encoded = "JVBERi0xLjcK";
    let value = mcp_runtime_tool_result(
        "read_pdf_chunk",
        false,
        ToolResult::ok(
            json!({"pdf_chunk": {"project":"agent:pdf:demo", "path":"report.pdf", "content_base64": encoded}}),
        ),
    );
    assert_eq!(
        value["_meta"]["webcodex/pdfChunk"]["content_base64"],
        encoded
    );
    assert!(!value["structuredContent"].to_string().contains(encoded));
    assert!(!value["content"].to_string().contains(encoded));
}

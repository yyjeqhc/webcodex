use super::*;

#[test]
fn read_project_artifact_uses_only_canonical_length_bound() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "read_project_artifact");
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert!(props.contains_key("length"));
    assert_eq!(props["length"]["maximum"], 65536);
    let expected_sha256 = &props["expected_sha256"];
    assert_eq!(expected_sha256["type"], "string");
    assert_eq!(expected_sha256["minLength"], 64);
    assert_eq!(expected_sha256["maxLength"], 64);
    assert_eq!(expected_sha256["pattern"], "^[0-9a-f]{64}$");
    assert!(
        !props.contains_key("max_bytes"),
        "read_project_artifact must not advertise the retired max_bytes alias"
    );
}

#[test]
fn read_project_artifact_metadata_schema_exposes_allow_missing() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "read_project_artifact_metadata");
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert!(props.contains_key("allow_missing"));
    assert!(
        spec.description.contains("allow_missing=true")
            && spec.description.contains("exists=false"),
        "description should explain successful missing assertions: {}",
        spec.description
    );
}

#[test]
fn artifact_upload_followup_descriptions_explain_required_path_binding() {
    let specs = registered_tool_specs();
    for name in [
        "artifact_upload_chunk",
        "artifact_upload_finish",
        "artifact_upload_abort",
    ] {
        let spec = spec_named(&specs, name);
        assert!(
            spec.description.contains("path is required")
                && spec.description.contains("artifact_upload_begin")
                && spec.description.contains("binds upload_id"),
            "{name}: {}",
            spec.description
        );
        let path_desc = spec.input_schema["properties"]["path"]["description"]
            .as_str()
            .unwrap();
        assert!(
            path_desc.contains("Required")
                && path_desc.contains("must exactly match the path used in artifact_upload_begin")
                && path_desc.contains("bind upload_id"),
            "{name}: {path_desc}"
        );
    }
}

#[test]
fn transfer_project_artifact_has_two_project_contract_and_no_payload_field() {
    let definition = lookup_tool_definition("transfer_project_artifact")
        .expect("transfer_project_artifact definition");
    assert_eq!(definition.metadata.effect, ToolEffect::Mutate);
    assert_eq!(definition.metadata.risk, ToolRisk::ProjectWrite);
    assert_eq!(definition.metadata.approval, ToolApprovalPolicy::Standard);
    assert_eq!(
        definition.metadata.authority,
        ToolAuthorityPolicy::RequireAll(&[PROJECT_READ, PROJECT_WRITE])
    );
    assert!(definition.requires_permission());
    assert!(
        !definition.metadata.requires_project,
        "two-project transfer has no singular generic project binding"
    );

    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "transfer_project_artifact");
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert_eq!(spec.input_schema["additionalProperties"], false);
    assert_eq!(
        spec.input_schema["required"],
        json!([
            "source_project",
            "source_path",
            "destination_project",
            "destination_path"
        ])
    );
    for field in [
        "source_project",
        "source_path",
        "destination_project",
        "destination_path",
        "overwrite",
    ] {
        assert!(props.contains_key(field), "{field}");
    }
    for forbidden in ["content_base64", "download_url", "upload_id"] {
        assert!(!props.contains_key(forbidden), "{forbidden}");
    }
    let output = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    for field in [
        "source_project",
        "source_path",
        "destination_project",
        "destination_path",
        "bytes",
        "sha256",
        "mime_type",
    ] {
        assert!(output.contains_key(field), "{field}");
    }
}

#[test]
fn accept_artifact_handoff_requires_destination_write_without_source_authority() {
    let definition = lookup_tool_definition("accept_artifact_handoff")
        .expect("accept_artifact_handoff definition");
    assert_eq!(definition.metadata.effect, ToolEffect::Mutate);
    assert_eq!(definition.metadata.risk, ToolRisk::ProjectWrite);
    assert_eq!(definition.metadata.approval, ToolApprovalPolicy::Standard);
    assert_eq!(
        definition.metadata.authority,
        ToolAuthorityPolicy::RequireAll(&[PROJECT_WRITE])
    );
    assert!(definition.requires_permission());
    assert!(definition.metadata.requires_project);

    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "accept_artifact_handoff");
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert_eq!(spec.input_schema["additionalProperties"], false);
    assert_eq!(
        spec.input_schema["required"],
        json!([
            "grant_id",
            "destination_project",
            "destination_path",
            "idempotency_key"
        ])
    );
    for field in [
        "grant_id",
        "destination_project",
        "destination_path",
        "overwrite",
        "idempotency_key",
    ] {
        assert!(props.contains_key(field), "{field}");
    }
    for forbidden in [
        "source_project",
        "source_path",
        "source_sha256",
        "content_base64",
        "download_url",
        "upload_id",
    ] {
        assert!(!props.contains_key(forbidden), "{forbidden}");
    }

    let output = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    for field in [
        "acceptance_id",
        "grant_id",
        "replayed",
        "destination_project",
        "destination_path",
        "bytes",
        "sha256",
        "mime_type",
        "provenance",
    ] {
        assert!(output.contains_key(field), "{field}");
    }
    let provenance = output["provenance"]["properties"].as_object().unwrap();
    for field in [
        "grant_id",
        "source_project",
        "source_path",
        "source_bytes",
        "source_sha256",
        "source_mime_type",
        "source_name",
    ] {
        assert!(provenance.contains_key(field), "{field}");
    }

    let call = ToolCall::from_tool_name(
        "accept_artifact_handoff",
        json!({
            "grant_id": "wc_handoff_mZmZmZmZmZmZmZmZ",
            "destination_project": "agent:destination:project",
            "destination_path": "artifacts/imported.bin",
            "overwrite": false,
            "idempotency_key": "accept-1"
        }),
    )
    .expect("accept_artifact_handoff parses");
    assert_eq!(call.tool_name(), "accept_artifact_handoff");
    assert_eq!(call.project(), Some("agent:destination:project"));
}

#[test]
fn project_artifact_is_compact_typed_project_read_facade() {
    let definition =
        lookup_tool_definition("project_artifact").expect("project_artifact definition");
    assert_eq!(definition.metadata.effect, ToolEffect::Observe);
    assert_eq!(definition.metadata.risk, ToolRisk::Read);
    assert_eq!(definition.metadata.approval, ToolApprovalPolicy::None);
    assert_eq!(definition.metadata.idempotency, ToolIdempotency::PureRead);
    assert_eq!(
        definition.metadata.authority,
        ToolAuthorityPolicy::Require(PROJECT_READ)
    );
    assert!(!definition.requires_permission());

    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "project_artifact");
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert_eq!(spec.input_schema["additionalProperties"], false);
    assert_eq!(
        spec.input_schema["required"],
        json!(["project", "path", "action"])
    );
    assert_eq!(
        props["action"]["enum"],
        json!(["metadata", "inspect", "image", "export"])
    );
    assert!(!props.contains_key("encoding"));
    assert!(spec.input_schema.get("allOf").is_none());
    assert!(ToolCall::from_tool_name(
        "project_artifact",
        json!({"project":"demo","path":"a.bin","action":"metadata","offset":0}),
    )
    .is_err());
    assert!(ToolCall::from_tool_name(
        "project_artifact",
        json!({"project":"demo","path":"a.bin","action":"inspect","offset":0,"length":1024}),
    )
    .is_ok());
    let output_props = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .expect("project_artifact output properties");
    for field in [
        "path",
        "exists",
        "bytes",
        "file_bytes",
        "sha256",
        "mime_type",
        "content_base64",
        "content_delivery",
        "suggested_call",
    ] {
        assert!(
            output_props.contains_key(field),
            "missing output field {field}"
        );
    }
    let suggested = &output_props["suggested_call"];
    assert_eq!(suggested["properties"]["tool"]["const"], "project_artifact");
    assert_eq!(
        suggested["properties"]["arguments"]["properties"]["action"]["const"],
        "inspect"
    );
    assert_eq!(
        suggested["properties"]["arguments"]["required"],
        json!([
            "project",
            "path",
            "action",
            "offset",
            "length",
            "expected_sha256"
        ])
    );
    assert!(spec.description.contains("not repeated inspect"));
    assert!(spec
        .description
        .contains("import_conversation_files_to_project"));
}

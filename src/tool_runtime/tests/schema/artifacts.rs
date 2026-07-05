use super::*;

#[test]
fn read_project_artifact_metadata_schema_exposes_allow_missing() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "read_project_artifact_metadata");
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert!(
        props.contains_key("allow_missing"),
        "read_project_artifact_metadata input schema must expose allow_missing"
    );
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

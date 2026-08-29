use super::*;

#[test]
fn openapi_unified_diff_request_rejects_codex_wrapper_and_uses_canonical_bound() {
    let spec = build_openapi_spec();
    let schema = &spec["components"]["schemas"]["ApplyUnifiedDiffRequest"];
    let diff = &schema["properties"]["diff"];
    let description = diff["description"]
        .as_str()
        .expect("ApplyUnifiedDiffRequest diff description");

    assert!(description
        .to_lowercase()
        .contains("raw standard unified diff"));
    assert!(description.contains("Codex apply_patch wrapper"));
    assert!(description.contains("*** Begin Patch"));
    assert_eq!(diff["maxLength"], MAX_UNIFIED_DIFF_BYTES);
    assert_eq!(
        schema["properties"]["deny_sensitive_paths"]["default"],
        true
    );
    assert!(schema["properties"].get("patch").is_none());
}

use super::*;

#[test]
fn session_tool_specs_describe_explicit_targeting() {
    let specs = registered_tool_specs();

    let desc = |name: &str| spec_named(&specs, name).description.to_lowercase();

    let summary_desc = desc("session_summary");
    for phrase in ["session ledger", "explicit session_id"] {
        assert!(
            summary_desc.contains(phrase),
            "session_summary description should mention {phrase}: {summary_desc}"
        );
    }

    let update = spec_named(&specs, "update_session_context");
    assert_eq!(
        update.input_schema["required"],
        serde_json::json!(["project", "session_id", "execution_context"])
    );
    assert_eq!(update.input_schema["additionalProperties"], false);
    assert_eq!(
        update.input_schema["properties"]["execution_context"]["additionalProperties"],
        false
    );
    assert!(
        update.input_schema["properties"]["execution_context"]["properties"]
            .get("resource")
            .is_some(),
        "update_session_context must expose the named SSH resource field"
    );
    let update_desc = update.description.to_lowercase();
    for phrase in [
        "authorized project",
        "exact session project",
        "cross-project escape is not supported",
        "store lock",
        "background writer",
        "success does not mean",
        "never falls back",
        "never creates",
    ] {
        assert!(
            update_desc.contains(phrase),
            "update_session_context description should mention {phrase}: {update_desc}"
        );
    }

    let handoff_desc = desc("session_handoff_summary");
    for phrase in [
        "session ledger",
        "explicit session_id",
        "ledger-derived validation",
        "bounded tails",
        "safe result metadata",
        "validation.parser.available",
    ] {
        assert!(
            handoff_desc.contains(phrase),
            "session_handoff_summary description should mention {phrase}: {handoff_desc}"
        );
    }

    let names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();
    for removed in [
        "bind_current_session",
        "current_session",
        "unbind_current_session",
    ] {
        assert!(
            !names.contains(&removed),
            "removed Session tool leaked into specs: {removed}"
        );
    }
}

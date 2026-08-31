use super::*;

#[test]
fn tool_specs_annotations_are_canonical_semantic_projections() {
    use crate::tool_runtime::metadata::{
        ToolApprovalPolicy, ToolEffect, ToolIdempotency, ToolRisk,
    };

    let specs = registered_tool_specs();
    for spec in &specs {
        let metadata = crate::tool_runtime::metadata::lookup_tool_metadata(&spec.name)
            .unwrap_or_else(|| panic!("{} missing metadata", spec.name));
        let annotations = spec
            .annotations
            .as_object()
            .unwrap_or_else(|| panic!("{} annotations must be an object", spec.name));
        for field in [
            "readOnlyHint",
            "destructiveHint",
            "idempotentHint",
            "openWorldHint",
        ] {
            assert!(
                annotations.contains_key(field),
                "{} missing annotation {}",
                spec.name,
                field
            );
        }
        assert_eq!(
            annotations["readOnlyHint"],
            metadata.effect.read_only_hint(),
            "{} readOnlyHint must derive from Effect",
            spec.name
        );
        assert_eq!(
            annotations["destructiveHint"], metadata.destructive,
            "{} destructiveHint must derive from canonical metadata",
            spec.name
        );
        assert_eq!(
            annotations["idempotentHint"],
            metadata.idempotency.mcp_hint(),
            "{} idempotentHint must derive from Idempotency",
            spec.name
        );
        assert_eq!(
            annotations["openWorldHint"], metadata.shell_like,
            "{} openWorldHint must remain independent from effect/approval",
            spec.name
        );
    }

    let cases = [
        ("read_file", ToolEffect::Observe, ToolIdempotency::PureRead),
        (
            "close_session",
            ToolEffect::Mutate,
            ToolIdempotency::DesiredState,
        ),
        (
            "coding_agent_start",
            ToolEffect::Execute,
            ToolIdempotency::Keyed,
        ),
        (
            "complete_session_message",
            ToolEffect::Mutate,
            ToolIdempotency::FencedReplay,
        ),
        (
            "write_project_file",
            ToolEffect::Mutate,
            ToolIdempotency::NonIdempotent,
        ),
    ];
    for (name, effect, idempotency) in cases {
        let metadata = crate::tool_runtime::metadata::lookup_tool_metadata(name).unwrap();
        assert_eq!(metadata.effect, effect, "{name}");
        assert_eq!(metadata.idempotency, idempotency, "{name}");
        let annotations = &spec_named(&specs, name).annotations;
        assert_eq!(
            annotations["readOnlyHint"],
            effect == ToolEffect::Observe,
            "{name}"
        );
        assert_eq!(
            annotations["idempotentHint"],
            idempotency.mcp_hint(),
            "{name}"
        );
    }

    let close = crate::tool_runtime::metadata::lookup_tool_metadata("close_session").unwrap();
    assert_eq!(close.approval, ToolApprovalPolicy::None);
    assert_eq!(close.risk, ToolRisk::SessionCollaborate);
    assert!(
        !crate::tool_runtime::tool_definition::runtime_tool_requires_permission("close_session")
    );

    let cancel =
        crate::tool_runtime::metadata::lookup_tool_metadata("coding_agent_cancel").unwrap();
    assert_eq!(cancel.effect, ToolEffect::Mutate);
    assert_eq!(cancel.risk, ToolRisk::RunControl);
    assert_eq!(cancel.approval, ToolApprovalPolicy::InheritFromStart);
    assert_eq!(cancel.idempotency, ToolIdempotency::DesiredState);
    assert_eq!(
        spec_named(&specs, "coding_agent_cancel").annotations["readOnlyHint"],
        false
    );
    assert_eq!(
        spec_named(&specs, "coding_agent_cancel").annotations["idempotentHint"],
        true
    );
    assert!(
        !crate::tool_runtime::tool_definition::runtime_tool_requires_permission(
            "coding_agent_cancel"
        )
    );

    assert_eq!(
        spec_named(&specs, "run_shell").annotations["openWorldHint"],
        true
    );
    assert_eq!(
        spec_named(&specs, "delete_project_files").annotations["destructiveHint"],
        true
    );
}

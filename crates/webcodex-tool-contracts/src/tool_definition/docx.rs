use super::*;
use crate::metadata::{ToolPathHint::Artifact, ToolRisk::Read, PROJECT_READ};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    adaptive_runtime_direct(
        model_spec(
            docx_read_definition("present_docx", ToolVisibility::ModelVisible, ToolIdempotency::NonIdempotent),
            "Open one authorized project-relative .docx in a dedicated read-only MCP App reader. Use for document viewing; present_work_result remains for coding progress. Unchanged/untracked files are supported; no Git or Workflow Session required. Pins file size/SHA-256 (maximum 10 MiB) and validates the DOCX package header. The App privately reads bounded segments, verifies the complete digest and limits ZIP expansion; changed files fail closed until explicitly reopened. Preserves common text, tables and images; Word-equivalent automatic pagination, editing, encrypted files and legacy .doc are unsupported. Repeat calls may create another Host card. Rendering requires an MCP Apps Host; never route complete document bytes through model Base64.",
        ),
        157,
        ToolDirectReason::Presentation,
    ),
    docx_read_definition("read_docx_chunk", ToolVisibility::ModelHidden, ToolIdempotency::PureRead),
];

const DOCX_AUDIT_FIELDS: &[ToolAuditResultField] = &[ToolAuditResultField::value("error_kind")];

const fn docx_read_definition(
    name: &'static str,
    visibility: ToolVisibility,
    idempotency: ToolIdempotency,
) -> ToolDefinition {
    def(
        name,
        ToolAuditPolicy::typed_fields(DOCX_AUDIT_FIELDS),
        visibility,
        TOOL_CATEGORY_ARTIFACT,
        Some(RunnerCapabilityRequirement::FileRead),
        TOOL_PROVIDER_CONTROL,
        ToolSemanticContract {
            effect: ToolEffect::Observe,
            risk: Read,
            approval: ToolApprovalPolicy::None,
            idempotency,
        },
        Some(PROJECT_READ),
        true,
        Artifact,
        false,
        false,
        ToolSessionEvidencePolicy::NONE,
    )
    .with_activity(
        ToolActivityPresentation::Transport,
        ToolActivityInteraction::NonMeaningful,
    )
}

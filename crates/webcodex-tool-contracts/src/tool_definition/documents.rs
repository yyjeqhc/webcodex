use super::*;
use crate::metadata::{ToolPathHint::Artifact, ToolRisk::Read, PROJECT_READ};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    adaptive_runtime_direct(
        model_spec(
            document_read_definition("present_pdf", ToolVisibility::ModelVisible, ToolIdempotency::NonIdempotent),
            "Open one PDF in a dedicated MCP App reader with no work activity, changes or collaboration panels. Use when the user asks to view a PDF; use present_work_result for substantial coding progress. Supply an authorized Project and project-relative path, including unchanged/untracked files; no Git or Session required. Pins size/SHA-256 (maximum 20 MiB), validates the PDF header, and never edits or exports the file. The App privately reads bounded chunks and fails if the version changes; reopen explicitly to select a new version. Rendering requires an MCP Apps Host. Repeat calls may create another Host card; retry inside the reader keeps the selected version.",
        ),
        156,
        ToolDirectReason::Presentation,
    ),
    document_read_definition("read_app_artifact_chunk", ToolVisibility::ModelHidden, ToolIdempotency::PureRead),
];

const DOCUMENT_AUDIT_FIELDS: &[ToolAuditResultField] = &[ToolAuditResultField::value("error_kind")];

const fn document_read_definition(
    name: &'static str,
    visibility: ToolVisibility,
    idempotency: ToolIdempotency,
) -> ToolDefinition {
    def(
        name,
        ToolAuditPolicy::typed_fields(DOCUMENT_AUDIT_FIELDS),
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

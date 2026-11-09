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
    ToolDeclaration {
        name,
        audit: ToolAuditPolicy::typed_fields(DOCUMENT_AUDIT_FIELDS),
        visibility,
        category: TOOL_CATEGORY_ARTIFACT,
        runner_capability: Some(RunnerCapabilityRequirement::FileRead),
        provider_id: TOOL_PROVIDER_CONTROL,
        semantic: ToolSemanticContract {
            effect: ToolEffect::Observe,
            risk: Read,
            approval: ToolApprovalPolicy::None,
            idempotency,
        },
        required_scope: Some(PROJECT_READ),
        requires_project: true,
        path_hint: Artifact,
        destructive: false,
        shell_like: false,
        session_evidence: ToolSessionEvidencePolicy::NONE,
    }
    .define()
    .with_activity(
        ToolActivityPresentation::Transport,
        ToolActivityInteraction::NonMeaningful,
    )
}

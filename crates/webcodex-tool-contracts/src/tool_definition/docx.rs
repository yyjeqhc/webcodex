use super::*;
use crate::metadata::{ToolPathHint::Artifact, ToolRisk::Read, PROJECT_READ};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    adaptive_runtime_direct(
        model_spec(
            docx_read_definition("present_docx", ToolVisibility::ModelVisible, ToolIdempotency::NonIdempotent),
            "Open one authorized project-relative .docx in a dedicated read-only MCP App reader. Use for document viewing; present_work_result remains for coding progress. Unchanged/untracked files are supported; no Git or Workflow Session required. Pins file size/SHA-256 (maximum 10 MiB) and validates the DOCX package header. The App privately reads bounded segments, verifies the complete digest and limits ZIP expansion; changed files fail closed until explicitly reopened. Preserves common text, tables and images; Word-equivalent automatic pagination, editing, encrypted files and legacy .doc are unsupported. Repeat calls may create another Host card. Rendering requires an MCP Apps Host; never route complete document bytes through model Base64.",
        ),
        158,
        ToolDirectReason::Presentation,
    ),
];

const DOCX_AUDIT_FIELDS: &[ToolAuditResultField] = &[ToolAuditResultField::value("error_kind")];

const fn docx_read_definition(
    name: &'static str,
    visibility: ToolVisibility,
    idempotency: ToolIdempotency,
) -> ToolDefinition {
    ToolDeclaration {
        name,
        audit: ToolAuditPolicy::typed_fields(DOCX_AUDIT_FIELDS),
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

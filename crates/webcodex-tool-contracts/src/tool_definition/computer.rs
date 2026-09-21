use super::RunnerCapabilityRequirement::FileWrite;
use super::ToolVisibility::ModelVisible;
use super::{
    def, model_spec, permission_risk, require_all_scopes, require_any_scopes, ToolDefinition,
    PERMISSION_RISK_WRITE, TOOL_CATEGORY_COMPUTER,
};
use crate::metadata::{
    ToolPathHint::{Artifact, None as NoPath},
    ToolRisk::{ComputerControl as ComputerControlRisk, ProjectWrite, Read},
    COMPUTER_CONTROL, COMPUTER_DISPLAY_READ, COMPUTER_LAUNCH, COMPUTER_READ, PROJECT_WRITE,
    TOOL_PROVIDER_CONTROL,
};

const COMPUTER_CONTROL_GATEWAY_SCOPES: &[&str] = &[COMPUTER_CONTROL, COMPUTER_LAUNCH];

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    model_spec(
        def(
            "computer_observe",
            super::ToolAuditPolicy::typed_semantic(
                super::ToolAuditSemanticResultPolicy::ComputerObservation,
            ),
            ModelVisible, TOOL_CATEGORY_COMPUTER, None, TOOL_PROVIDER_CONTROL,
            super::ToolSemanticContract { effect: super::ToolEffect::Observe, risk: Read, approval: super::ToolApprovalPolicy::None, idempotency: super::ToolIdempotency::PureRead },
            Some(COMPUTER_READ), false, NoPath, false, false, super::ToolSessionEvidencePolicy::NONE,
        ),
        "Guaranteed read-only Computer observation gateway. Use the closed action vocabulary for targets, windows, displays, applications, Accessibility status/tree/search/state, window/display snapshots, or clipboard text. Exact action scopes and Runner capabilities are enforced before dispatch; opaque ephemeral identities, stale-handle failure, traversal/image/clipboard bounds, and snapshot-generation semantics remain unchanged. No action can activate, launch, focus, type, move/click the pointer, write the clipboard, save a project artifact, use shell fallback, or retry an uncertain effect.",
    ),
    require_any_scopes(
        permission_risk(
            model_spec(
                def(
                    "computer_control",
                    super::ToolAuditPolicy::typed_semantic(
                        super::ToolAuditSemanticResultPolicy::ComputerControl,
                    ),
                    ModelVisible, TOOL_CATEGORY_COMPUTER, None, TOOL_PROVIDER_CONTROL,
                    super::ToolSemanticContract { effect: super::ToolEffect::Execute, risk: ComputerControlRisk, approval: super::ToolApprovalPolicy::Standard, idempotency: super::ToolIdempotency::NonIdempotent },
                    None, false, NoPath, true, false, super::ToolSessionEvidencePolicy::NONE,
                ),
                "Effectful Computer control gateway with a closed action vocabulary: launch_application, activate_window, press, focus, scroll_to_element, key, input_text, pointer_move, pointer_click, and write_clipboard. Each action keeps its exact scopes, permission/session semantics, Runner capability fence, native validation, execution certainty, and observation-first recovery. The outer ToolDefinition is a worst-case effect annotation only; action-sensitive canonical governance resolves exact authority before any effect. No arbitrary argv/path/script input, implicit focus/activation, shell fallback, or blind retry after an uncertain effect.",
            ),
            PERMISSION_RISK_WRITE,
        ),
        COMPUTER_CONTROL_GATEWAY_SCOPES,
    ),
    require_all_scopes(
        model_spec(
            def(
                "computer_save_display_snapshot",
                super::ToolAuditPolicy::typed_fields(&[
                    super::ToolAuditResultField::value("project"),
                    super::ToolAuditResultField::value("path"),
                    super::ToolAuditResultField::value("client_id"),
                    super::ToolAuditResultField::value("display_id"),
                    super::ToolAuditResultField::value("source_width"),
                    super::ToolAuditResultField::value("source_height"),
                    super::ToolAuditResultField::value("width"),
                    super::ToolAuditResultField::value("height"),
                    super::ToolAuditResultField::value("mime_type"),
                    super::ToolAuditResultField::value("file_bytes"),
                    super::ToolAuditResultField::value("saved"),
                ]),
                ModelVisible,
                TOOL_CATEGORY_COMPUTER,
                Some(FileWrite),
                TOOL_PROVIDER_CONTROL,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: ProjectWrite,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::NonIdempotent,
                },
                Some(PROJECT_WRITE),
                true,
                Artifact,
                false,
                false,
                super::ToolSessionEvidencePolicy::NONE,
            ),
            "Save one exact display snapshot as a create-only project artifact without returning image bytes. Reuses computer_observe(action=snapshot_display) downscale semantics and requires project:write, computer:read, and computer:display_read. No overwrite or encoding control. Unknown writes require artifact-metadata reconciliation before retry.",
        ),
        &[PROJECT_WRITE, COMPUTER_READ, COMPUTER_DISPLAY_READ],
    ),
    require_all_scopes(
        model_spec(
            def(
                "computer_save_snapshot",
                super::ToolAuditPolicy::typed_fields(&[
                    super::ToolAuditResultField::value("project"),
                    super::ToolAuditResultField::value("path"),
                    super::ToolAuditResultField::value("client_id"),
                    super::ToolAuditResultField::value("surface_id"),
                    super::ToolAuditResultField::value("source_width"),
                    super::ToolAuditResultField::value("source_height"),
                    super::ToolAuditResultField::presence("region_present", "region"),
                    super::ToolAuditResultField::value("width"),
                    super::ToolAuditResultField::value("height"),
                    super::ToolAuditResultField::value("mime_type"),
                    super::ToolAuditResultField::value("file_bytes"),
                    super::ToolAuditResultField::value("saved"),
                ]),
                ModelVisible,
                TOOL_CATEGORY_COMPUTER,
                Some(FileWrite),
                TOOL_PROVIDER_CONTROL,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: ProjectWrite,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::NonIdempotent,
                },
                Some(PROJECT_WRITE),
                true,
                Artifact,
                false,
                false,
                super::ToolSessionEvidencePolicy::NONE,
            ),
            "Save one exact window snapshot as a create-only project artifact without returning image bytes. Reuses computer_observe(action=snapshot_window) region/downscale semantics and requires computer:read plus project:write. No overwrite or encoding control. Unknown writes require artifact-metadata reconciliation before retry.",
        ),
        &[PROJECT_WRITE, COMPUTER_READ],
    ),
];

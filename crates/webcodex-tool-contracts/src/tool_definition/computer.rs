use super::RunnerCapabilityRequirement::FileWrite;
use super::ToolVisibility::ModelVisible;
use super::{
    def, model_spec, permission_risk, require_all_scopes, require_any_scopes, ToolDefinition,
    PERMISSION_RISK_WRITE, TOOL_CATEGORY_COMPUTER,
};
use crate::metadata::{
    ToolPathHint::{Artifact, None as NoPath},
    ToolRisk::{ComputerControl as ComputerControlRisk, ProjectWrite, Read},
    COMPUTER_CONTROL, COMPUTER_LAUNCH, COMPUTER_READ, PROJECT_WRITE, TOOL_PROVIDER_CONTROL,
};

const COMPUTER_CONTROL_GATEWAY_SCOPES: &[&str] = &[COMPUTER_CONTROL, COMPUTER_LAUNCH];

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    model_spec(
        def(
            "observe_computer",
            super::ToolAuditPolicy::typed_semantic(
                super::ToolAuditSemanticResultPolicy::ComputerObservation,
            ),
            ModelVisible, TOOL_CATEGORY_COMPUTER, None, TOOL_PROVIDER_CONTROL,
            super::ToolSemanticContract { effect: super::ToolEffect::Observe, risk: Read, approval: super::ToolApprovalPolicy::None, idempotency: super::ToolIdempotency::PureRead },
            Some(COMPUTER_READ), false, NoPath, false, false, super::ToolSessionEvidencePolicy::NONE,
        ),
        "Read-only Computer gateway: targets/windows/displays/applications, accessibility_status/accessibility_tree/find_elements/element_state, snapshot_window/snapshot_display, read_clipboard. If action support is unclear, use element_state with exact client_id/surface_id/element_id; can_press/can_focus/can_input_text guide the corresponding control_computer actions. Native eligibility grants no permission/capability; control revalidates. Role/readable value alone is insufficient. Query again only when changed state affects the next decision. element_state preserves observation_generation; stale IDs require fresh tree/search. Tree/search replaces all IDs on that surface, including root_element_id. For truncated searches, choose a branch in a fresh tree and query its latest root_element_id. Scoped reads require an updated Runner and verified scope receipt. Limits: relative depth 8, nodes 256, absolute lineage 32; native deadlines apply. Truncated zero results are inconclusive; unseen siblings beyond the node cap are not paginated. Windows ComboBox value observation grants no focus/input. Exact action scopes, Runner capabilities, protected-content, image/clipboard bounds and snapshot-generation guards apply. No activation, launch, control input, pointer/clipboard writes, project artifact save, shell fallback, or uncertain-effect retry.",
    ),
    require_any_scopes(
        permission_risk(
            model_spec(
                def(
                    "control_computer",
                    super::ToolAuditPolicy::typed_semantic(
                        super::ToolAuditSemanticResultPolicy::ComputerControl,
                    ),
                    ModelVisible, TOOL_CATEGORY_COMPUTER, None, TOOL_PROVIDER_CONTROL,
                    super::ToolSemanticContract { effect: super::ToolEffect::Execute, risk: ComputerControlRisk, approval: super::ToolApprovalPolicy::Standard, idempotency: super::ToolIdempotency::NonIdempotent },
                    None, false, NoPath, true, false, super::ToolSessionEvidencePolicy::NONE,
                ),
                "Effectful Computer gateway: launch_application, activate_window, press, focus, scroll_to_element, key, input_text, pointer_move, pointer_click, write_clipboard. For unclear support, query observe_computer(action=element_state) on exact client_id/surface_id/element_id: can_press/can_focus/can_input_text advise corresponding actions; false means avoid unchanged retries, true grants no authority. Windows press needs InvokePattern; focus/input need foreground AXTextField; input needs exact focus and writable empty value. key accepts navigation enums; Windows also supports Ctrl+J/Ctrl+P as key=j/p,modifiers=[control] exactly, requiring computer_letter_shortcuts as well as computer_key_input. Other letter chords and shortcut strings are unsupported. These are virtual-key shortcuts, not text or guaranteed application commands. Shift+Tab: key=tab,modifiers=[shift]. Windows option=Alt, command unsupported; option+Tab, option+Escape, control+Escape rejected. macOS uses named modifiers. Surface must already be foreground/focused, with unprotected focus and native preflight; release held keys before re-observing. press/input_text may use observe_after={action:accessibility_tree,max_depth?,max_nodes?}: requires computer:read and accessibility capability before acting, pins Runner/window, returns fresh bounded output.post_observation={success,output,error?}. Tree refresh invalidates old IDs; not UI-settled. Read failure preserves action success; uncertain action skips read. Existing action scopes, permissions, capabilities, identities and snapshot guards apply. No implicit focus/activation, script fallback or blind effect replay.",
            ),
            PERMISSION_RISK_WRITE,
        ),
        COMPUTER_CONTROL_GATEWAY_SCOPES,
    ),
    require_all_scopes(
        model_spec(
            def(
                "save_computer_snapshot",
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
            "Save one exact window snapshot as a create-only project artifact without returning image bytes. Reuses observe_computer(action=snapshot_window) region/downscale semantics and requires computer:read plus project:write. No overwrite or encoding control. Unknown writes require artifact-metadata reconciliation before retry.",
        ),
        &[PROJECT_WRITE, COMPUTER_READ],
    ),
];

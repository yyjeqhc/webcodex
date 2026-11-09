use super::RunnerCapabilityRequirement::FileWrite;
use super::ToolVisibility::ModelVisible;
use super::{
    control_def, model_spec, permission_risk, require_all_scopes, require_any_scopes,
    ToolDeclaration, ToolDefinition, PERMISSION_RISK_WRITE, TOOL_CATEGORY_COMPUTER,
};
use crate::metadata::{
    ToolPathHint::Artifact,
    ToolRisk::{ComputerControl as ComputerControlRisk, ProjectWrite, Read},
    COMPUTER_CONTROL, COMPUTER_LAUNCH, COMPUTER_READ, PROJECT_WRITE, TOOL_PROVIDER_CONTROL,
};

const COMPUTER_CONTROL_GATEWAY_SCOPES: &[&str] = &[COMPUTER_CONTROL, COMPUTER_LAUNCH];

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    model_spec(
        control_def(
            "observe_computer",
            super::ToolAuditPolicy::typed_semantic(
                super::ToolAuditSemanticResultPolicy::ComputerObservation,
            ),
            ModelVisible,
            TOOL_CATEGORY_COMPUTER,
            super::ToolSemanticContract { effect: super::ToolEffect::Observe, risk: Read, approval: super::ToolApprovalPolicy::None, idempotency: super::ToolIdempotency::PureRead },
            Some(COMPUTER_READ),
            false,
        ),
        "Read-only Computer gateway: targets/windows/displays/applications, accessibility_status/accessibility_tree/find_elements/element_state, snapshot_window/snapshot_display, read_clipboard. Use exact client_id/surface_id/element_id with element_state: can_press/can_focus/can_input_text/can_select guide control_computer actions, never authority. Role/value alone is insufficient; control revalidates permissions/capabilities. Recheck only when changed state affects the next decision. State preserves observation_generation; stale IDs need fresh tree/search. Tree/search replaces all surface IDs including root_element_id. For truncated search, choose a fresh tree branch and its latest root_element_id. Scoped reads require updated Runner/verified scope receipt. Bounds: depth 8, nodes 256, absolute lineage 32; native deadlines. Truncated zero results are inconclusive; capped siblings are not paginated. Windows element_state scroll fields horizontal_percent/vertical_percent require computer_scroll: null axis=non-scrollable, null object=unsupported/protected, missing=unknown. Positions grant no authority. ComboBox value grants no focus/input. Exact scopes, Runner capabilities, protected-content, image/clipboard bounds and snapshot-generation guards apply. No activation/launch/control input, pointer/clipboard writes, project artifact save, shell fallback or uncertain-effect retry.",
    ).with_discovery_keywords(&["desktop", "gui", "mac", "macos", "windows", "screen", "screenshot", "observe", "inspect", "view", "桌面", "屏幕"]),
    require_any_scopes(
        permission_risk(
            model_spec(
                control_def(
                    "control_computer",
                    super::ToolAuditPolicy::typed_semantic(
                        super::ToolAuditSemanticResultPolicy::ComputerControl,
                    ),
                    ModelVisible,
                    TOOL_CATEGORY_COMPUTER,
                    super::ToolSemanticContract { effect: super::ToolEffect::Execute, risk: ComputerControlRisk, approval: super::ToolApprovalPolicy::Standard, idempotency: super::ToolIdempotency::NonIdempotent },
                    None,
                    true,
                ),
                "Effectful Computer gateway; choose exact schema action/target. element_state can_press/can_focus/can_input_text/can_select guide actions, never authority; avoid unchanged false retries. Windows press needs InvokePattern; focus/input need foreground AXTextField; input needs exact focus, writable empty value. select replaces selection (computer_selection+computer_control); missing selected is unknown. scroll targets one exact Windows ScrollPattern container (computer_scroll+computer_control): one small/large increment; omitted/null=small. Distances vary. Re-observe; position_before/position_after do not prove UI stability/completion. key uses schema names: Windows computer_key_input, extra keys/chords computer_key_chords, legacy computer_letter_shortcuts only Ctrl+J/Ctrl+P; macOS navigation only. Keys are not text/app commands. Shift+Tab: key=tab,modifiers=[shift]. Windows option=Alt; command, option+Tab/option+Escape/control+Escape/control+option+Delete denied. Keys need foreground/focused surface, unprotected focus; release held keys and re-observe. Only press/input_text/scroll accept observe_after={action:accessibility_tree,max_depth?,max_nodes?}; computer:read and preflight pin Runner/window. post_observation read failure preserves action success; uncertain action skips read. Tree replaces IDs; read is not UI-settled. Scopes/permissions/capabilities/identity/snapshot guards apply. Local pause: resume locally; observe allowed. No implicit focus/activation, shell bypass, scripts or old/uncertain-effect replay.",
            ).with_discovery_keywords(&["desktop", "gui", "mac", "macos", "windows", "operate", "control", "click", "mouse", "keyboard", "桌面", "鼠标", "键盘"]),
            PERMISSION_RISK_WRITE,
        ),
        COMPUTER_CONTROL_GATEWAY_SCOPES,
    ),
    require_all_scopes(
        model_spec(
            ToolDeclaration {
                name: "save_computer_snapshot",
                audit: super::ToolAuditPolicy::typed_fields(&[
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
                visibility: ModelVisible,
                category: TOOL_CATEGORY_COMPUTER,
                runner_capability: Some(FileWrite),
                provider_id: TOOL_PROVIDER_CONTROL,
                semantic: super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: ProjectWrite,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::NonIdempotent,
                },
                required_scope: Some(PROJECT_WRITE),
                requires_project: true,
                path_hint: Artifact,
                destructive: false,
                shell_like: false,
                session_evidence: super::ToolSessionEvidencePolicy::NONE,
            }.define(),
            "Save one exact window snapshot as a create-only project artifact without returning image bytes. Reuses observe_computer(action=snapshot_window) region/downscale semantics and requires computer:read plus project:write. No overwrite or encoding control. Unknown writes require artifact-metadata reconciliation before retry.",
        ),
        &[PROJECT_WRITE, COMPUTER_READ],
    ),
];

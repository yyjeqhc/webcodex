use super::ToolVisibility::ModelVisible;
use super::{
    control_def, model_spec, permission_risk, require_any_scopes, ToolDefinition,
    PERMISSION_RISK_BROWSER_CONTROL, TOOL_CATEGORY_BROWSER,
};
use crate::metadata::{
    ToolRisk::{BrowserControl as BrowserControlRisk, Read},
    BROWSER_CONTROL, BROWSER_LAUNCH, BROWSER_READ,
};

const BROWSER_ACT_GATEWAY_SCOPES: &[&str] = &[BROWSER_CONTROL, BROWSER_LAUNCH];
#[cfg(test)]
#[path = "browser_continuity_tests.rs"]
mod continuity_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_model_surface_metrics_are_bounded() {
        assert_eq!(DEFINITIONS.len(), 2);
        let observe = DEFINITIONS[0]
            .model_spec
            .expect("observe_browser model spec");
        let act = DEFINITIONS[1]
            .model_spec
            .expect("control_browser model spec");
        let observe_schema_bytes =
            serde_json::to_vec(&crate::input_schema_for_tool("observe_browser"))
                .unwrap()
                .len();
        let act_schema_bytes = serde_json::to_vec(&crate::input_schema_for_tool("control_browser"))
            .unwrap()
            .len();
        let combined_schema_bytes = observe_schema_bytes + act_schema_bytes;
        let combined_description_bytes = observe.description.len() + act.description.len();
        println!(
            "browser_surface_metrics observe_schema_bytes={observe_schema_bytes} act_schema_bytes={act_schema_bytes} combined_schema_bytes={combined_schema_bytes} combined_description_bytes={combined_description_bytes}"
        );
        // Correctness-bearing branch schemas stay explicit; common semantics belong in
        // canonical descriptions rather than duplicated prose in each oneOf branch.
        assert!(observe_schema_bytes <= 8 * 1024);
        assert!(act_schema_bytes <= 16 * 1024);
        assert!(combined_description_bytes <= 2 * 1024);
        assert_eq!(
            DEFINITIONS[1].host_orchestration.concurrency,
            super::super::ToolHostConcurrencyHint::Sequential
        );
        assert_eq!(
            DEFINITIONS[1].host_orchestration.native_batch_field,
            Some("operations")
        );
    }
}

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    model_spec(
        control_def(
            "observe_browser",
            super::ToolAuditPolicy::typed_semantic(
                super::ToolAuditSemanticResultPolicy::BrowserObservation,
            ),
            ModelVisible,
            TOOL_CATEGORY_BROWSER,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Observe,
                risk: Read,
                approval: super::ToolApprovalPolicy::None,
                idempotency: super::ToolIdempotency::PureRead,
            },
            Some(BROWSER_READ),
            false,
        ),
        "Read-only Browser; needs browser:read and capabilities. discover lists explicit Share offers; browsers/pages lists attached runtimes. New tabs need Share then attach. stale_element: snapshot; stale_attachment: restore Share. surface maps exact native window, also needs computer:read; ambiguity fails closed. IDs are process-local. browsers/user_handoff recovers live hold/fence. Query AND-filters semantic fields over <=4352 nodes before paging. Bounds/truncation make absence inconclusive. New snapshot stales old ids; element_id/actions admit effects. since_cursor returns later diagnostics. No scripts/selectors/CDP.",
    ),
    require_any_scopes(
        permission_risk(
            model_spec(
                control_def(
                    "control_browser",
                    super::ToolAuditPolicy::typed_semantic(
                        super::ToolAuditSemanticResultPolicy::BrowserControl,
                    ),
                    ModelVisible,
                    TOOL_CATEGORY_BROWSER,
                    super::ToolSemanticContract {
                        effect: super::ToolEffect::Execute,
                        risk: BrowserControlRisk,
                        approval: super::ToolApprovalPolicy::Standard,
                        idempotency: super::ToolIdempotency::NonIdempotent,
                    },
                    None,
                    true,
                )
                .with_host_orchestration_hint(
                    super::ToolHostOrchestrationHint::sequential()
                        .with_native_batch_field("operations"),
                ),
                "Use opaque ids and admitted actions. launch needs browser:launch: managed opens a named visible persistent profile; attach consumes a live offer. Other effects need browser:control; uploads also project:read and a same-Runner file. begin_user_handoff also needs computer:read+runtime:read, exact Browser/authorized session_id/open question-or-todo message_id and browser_user_handoff capability. Requires owned visible managed Browser and native surface. Returns handoff_fence/surface_id and <=10min bound capped by Browser lifetime, never renewed. Input stays blocked after deadline until exact finish then fresh snapshot, or Browser close. Reply/ACK grants no authority or release. Finish needs original Session/Browser/fence, revokes old elements and requires fresh snapshot. Duplicate begin recovers same hold; duplicate finish rejects. Lost acknowledgement: observe browsers/user_handoff; never blindly replay. close_browser closes owned processes or detaches external. input_text inserts; select_option/set_value use native controls. select_choice chooses 1..4 semantic levels; set_date sets YYYY-MM[-DD]; both verify internally. Batch: 1..32 effects on one Browser/page/snapshot; stops on rejection, document change, uncertainty or budget. completed_count is known progress; remaining_count definitely unstarted; missing counts unknown. Never replay outcome_unknown. Snapshot to verify and after structural changes.",
            ),
            PERMISSION_RISK_BROWSER_CONTROL,
        ),
        BROWSER_ACT_GATEWAY_SCOPES,
    ),
];

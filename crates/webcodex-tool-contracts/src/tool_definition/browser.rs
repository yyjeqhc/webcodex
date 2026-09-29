use super::ToolVisibility::ModelVisible;
use super::{
    def, model_spec, permission_risk, require_any_scopes, ToolDefinition,
    PERMISSION_RISK_BROWSER_CONTROL, TOOL_CATEGORY_BROWSER,
};
use crate::metadata::{
    ToolPathHint::None as NoPath,
    ToolRisk::{BrowserControl as BrowserControlRisk, Read},
    BROWSER_CONTROL, BROWSER_LAUNCH, BROWSER_READ, TOOL_PROVIDER_CONTROL,
};

const BROWSER_ACT_GATEWAY_SCOPES: &[&str] = &[BROWSER_CONTROL, BROWSER_LAUNCH];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_model_surface_metrics_are_bounded() {
        assert_eq!(DEFINITIONS.len(), 2);
        let observe = DEFINITIONS[0]
            .model_spec
            .expect("browser_observe model spec");
        let act = DEFINITIONS[1].model_spec.expect("browser_act model spec");
        let observe_schema_bytes =
            serde_json::to_vec(&crate::input_schema_for_tool("browser_observe"))
                .unwrap()
                .len();
        let act_schema_bytes = serde_json::to_vec(&crate::input_schema_for_tool("browser_act"))
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
    }
}

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    model_spec(
        def(
            "browser_observe",
            super::ToolAuditPolicy::typed_semantic(
                super::ToolAuditSemanticResultPolicy::BrowserObservation,
            ),
            ModelVisible,
            TOOL_CATEGORY_BROWSER,
            None,
            TOOL_PROVIDER_CONTROL,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Observe,
                risk: Read,
                approval: super::ToolApprovalPolicy::None,
                idempotency: super::ToolIdempotency::PureRead,
            },
            Some(BROWSER_READ),
            false,
            NoPath,
            false,
            false,
            super::ToolSessionEvidencePolicy::NONE,
        ),
        "Guaranteed read-only Browser observation gateway with the closed actions targets, browsers, pages, snapshot, screenshot, console, network, and diagnostics. Browser/Page/Element identities are opaque and process-local. Snapshot auto mode compacts large pages to admitted controls and semantic choices such as select options; full/interactive and bounded max_nodes/max_depth are explicit overrides. An actionable node includes element_id and actions, the only browser_act effects it admits. Diagnostics supports a monotonic since_cursor delta for new console/network activity, with summarized new errors/warnings/4xx/5xx/failures; include_all flags expose the matching retained events. Projections stay bounded and report truncation. Screenshots use the shared native-image delivery contract at the MCP boundary. Exact Runner capability and browser:read authority are checked before dispatch. No effect, process launch, arbitrary protocol input, script execution, profile attachment, or shell fallback is available here.",
    ),
    require_any_scopes(
        permission_risk(
            model_spec(
                def(
                    "browser_act",
                    super::ToolAuditPolicy::typed_semantic(
                        super::ToolAuditSemanticResultPolicy::BrowserControl,
                    ),
                    ModelVisible,
                    TOOL_CATEGORY_BROWSER,
                    None,
                    TOOL_PROVIDER_CONTROL,
                    super::ToolSemanticContract {
                        effect: super::ToolEffect::Execute,
                        risk: BrowserControlRisk,
                        approval: super::ToolApprovalPolicy::Standard,
                        idempotency: super::ToolIdempotency::NonIdempotent,
                    },
                    None,
                    false,
                    NoPath,
                    true,
                    false,
                    super::ToolSessionEvidencePolicy::NONE,
                ),
                "Browser effects use opaque ids; no selectors or scripts. launch needs browser:launch; effects need browser:control; uploads also need project:read and a same-Runner project-relative regular file. Use only snapshot-admitted actions. Batch runs 1..32 ordered input_text/select_option/set_value/click/upload_file operations on one Browser/page/current snapshot, with freshness checks between effects and one final bounded settle. Rejection, document change, uncertainty or budget exhaustion stops dispatch without retry. completed_count records known effects; stopped_at_index is zero-based; stopped_execution_state preserves the stopped action's certainty. remaining_count counts definitely unstarted operations. Missing counts mean unknown progress. Ordinary effects preserve sibling ids; navigation, document replacement and new snapshots stale them. Follow needs_snapshot/recovery with observation, never blindly retry outcome_unknown or stability=false. Observe after structural changes; snapshot to verify filling.",
            ),
            PERMISSION_RISK_BROWSER_CONTROL,
        ),
        BROWSER_ACT_GATEWAY_SCOPES,
    ),
];

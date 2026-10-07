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
        def(
            "observe_browser",
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
        "Read-only Browser. discover lists only tabs explicitly shared in the local Chrome extension; it never attaches. surface resolves one exact live Browser window to an opaque Computer surface, requiring browser:read plus computer:read; missing or ambiguous windows fail closed. Others require browser:read and exact capabilities. Browser/page/element ids are opaque and process-local. Snapshot query AND-filters text/role/group/section/fields_only over at most 4352 source nodes before pagination; auto keeps text with query, otherwise compacts large pages. max_nodes/max_depth bound output. Query ids share one fresh generation; every snapshot stales older ids. truncated means absence is inconclusive. element_id and actions authorize only the listed effects. Diagnostics since_cursor returns new activity. Screenshots use native-image delivery. No effects, arbitrary CDP, scripts, selectors or shell fallback.",
    ),
    require_any_scopes(
        permission_risk(
            model_spec(
                def(
                    "control_browser",
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
                )
                .with_host_orchestration_hint(
                    super::ToolHostOrchestrationHint::sequential()
                        .with_native_batch_field("operations"),
                ),
                "Effects use opaque ids; no selectors/scripts. launch requires browser:launch: ephemeral is headless/temporary; mode=managed opens a named visible persistent owned profile. attach consumes a live discover offer under browser:control. close_browser closes owned processes; external Browsers only detach. All other effects require browser:control; uploads also require project:read and a same-Runner project-relative regular file. Use snapshot-admitted actions. set_value replaces native text/structured values; input_text inserts at the caret. Batch runs 1..32 ordered input_text/select_option/set_value/click/upload_file operations on one Browser/page/current snapshot, checks freshness between effects and settles once. Rejection, document change, uncertainty or budget exhaustion stops without retry. completed_count is known progress; stopped_at_index is zero-based; stopped_execution_state preserves certainty; remaining_count is definitely unstarted. Missing counts mean unknown. Navigation/replacement/new snapshots stale element ids. Observe recovery, never replay outcome_unknown or stability=false; snapshot to verify changes.",
            ),
            PERMISSION_RISK_BROWSER_CONTROL,
        ),
        BROWSER_ACT_GATEWAY_SCOPES,
    ),
];

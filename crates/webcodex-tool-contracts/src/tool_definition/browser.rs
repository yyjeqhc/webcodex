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
                "Use opaque ids and snapshot-admitted actions; no selectors/scripts. launch needs browser:launch: ephemeral is headless/temporary; managed opens a named visible persistent profile. attach consumes a live discover offer. close_browser closes owned processes or detaches external Browsers. Other effects need browser:control; uploads also need project:read and a same-Runner project-relative regular file. set_value replaces native values; input_text inserts; select_option is native select. select_choice(choice_path) selects 1..4 custom levels; set_date(value) sets YYYY-MM[-DD]; each discovers, chooses and verifies in one bounded operation. Batch: 1..32 admitted operations on one Browser/page/snapshot; check freshness between effects, settle once. Stop on rejection, document change, uncertainty or budget. completed_count is known progress; stopped_at_index is zero-based; stopped_execution_state owns certainty; remaining_count is definitely unstarted. Missing counts are unknown. Navigation/replacement/new snapshots stale ids. Observe recovery; never replay outcome_unknown or unstable results; snapshot to verify.",
            ),
            PERMISSION_RISK_BROWSER_CONTROL,
        ),
        BROWSER_ACT_GATEWAY_SCOPES,
    ),
];

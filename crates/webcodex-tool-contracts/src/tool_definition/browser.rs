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
        "Guaranteed read-only Browser observation gateway with the closed actions targets, browsers, pages, snapshot, screenshot, console, network, and diagnostics. Browser/Page/Element identities are opaque and process-local. Snapshot defaults to adaptive auto mode, compacting large pages to actionable controls; full/interactive mode plus bounded max_nodes/max_depth are explicit overrides. Diagnostics supports a monotonic since_cursor delta so callers can request only new console/network activity, with summarized new errors/warnings/4xx/5xx/failures; include_all flags expose the matching retained bounded events. All projections remain bounded and report truncation explicitly. Screenshots use the shared native-image delivery contract at the MCP boundary. Exact Runner capability and browser:read authority are checked before dispatch. No effect, process launch, arbitrary protocol input, script execution, profile attachment, or shell fallback is available here.",
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
                "Effectful Browser gateway for launch, new_page, navigate, reload, click, input_text, select_option, set_value, upload_file, key, clear_diagnostics, close_page, and close_browser. Authority is checked before dispatch: launch needs browser:launch, upload_file needs browser:control plus project:read, and other effects need browser:control. Successful page/control effects include bounded post-effect stability observation from document/DOM and network quiet signals; stability=false is observational and never makes a completed effect retry-safe. Reload/navigation stale prior element ids; clear_diagnostics uses a CDP barrier before reset. Uploads accept one same-Runner project-relative regular file. Effects preserve not_started/completed/outcome_unknown certainty and never blindly retry uncertain actions. No arbitrary protocol, executable, profile, remote endpoint, or script input is accepted.",
            ),
            PERMISSION_RISK_BROWSER_CONTROL,
        ),
        BROWSER_ACT_GATEWAY_SCOPES,
    ),
];

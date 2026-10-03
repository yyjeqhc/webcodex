use super::*;
use crate::metadata::{ToolPathHint::None as NoPath, ToolRisk::Read};

const fn resource_tool(
    name: &'static str,
    description: &'static str,
    rank: u16,
    reason: ToolDirectReason,
) -> ToolDefinition {
    adaptive_runtime_direct(
        model_spec(
            require_any_scopes(
                def(
                    name,
                    ToolAuditPolicy::TYPED_CANONICAL.session_input(
                        ToolAuditSessionInputPolicy::OmitTopLevel(&["uri", "query"]),
                    ),
                    ToolVisibility::ModelVisible,
                    TOOL_CATEGORY_PROJECT,
                    None,
                    TOOL_PROVIDER_CONTROL,
                    ToolSemanticContract {
                        effect: ToolEffect::Observe,
                        risk: Read,
                        approval: ToolApprovalPolicy::None,
                        idempotency: ToolIdempotency::PureRead,
                    },
                    None,
                    false,
                    NoPath,
                    false,
                    false,
                    ToolSessionEvidencePolicy::NONE,
                ),
                &[
                    crate::metadata::PROJECT_READ,
                    crate::metadata::COMMUNICATION_READ,
                ],
            )
            .with_activity(
                ToolActivityPresentation::Support,
                ToolActivityInteraction::NonMeaningful,
            ),
            description,
        ),
        rank,
        reason,
    )
}

// Authorization is selected by resource kind inside the canonical service. A common
// project/communication/session scope here would incorrectly disable other domains.
pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    resource_tool("open_webcodex_workbench", "Choose Runner, Project and optional Session. Explicit UI action attaches reauthorized context after Host acknowledgement, or offers copyable text. No Session creation, messages or execution.", 152, ToolDirectReason::Presentation),
    resource_tool("search_webcodex_resources", "Find authorized links: client_id filters Projects; files need project, artifacts need project+session_id. Literal query precedes paging; incomplete sources stay explicit. Links grant no authority.", 154, ToolDirectReason::CoreWorkflow),
    resource_tool("read_webcodex_resource", "Read the latest bounded content or metadata of one exact webcodex-resource:// reference. Re-authorizes the original domain; pinned project roots, deleted paths and inaccessible Goals fail closed. Artifacts reuse file identity; historical observations are not current facts.", 153, ToolDirectReason::CoreWorkflow),
];

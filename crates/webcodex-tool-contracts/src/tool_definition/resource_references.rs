use super::*;
use crate::metadata::{ToolPathHint::None as NoPath, ToolRisk::Read};

const fn resource_tool(name: &'static str, description: &'static str, rank: u16) -> ToolDefinition {
    adaptive_runtime_direct(model_spec(require_any_scopes(def(
        name, ToolAuditPolicy::TYPED_CANONICAL.session_input(ToolAuditSessionInputPolicy::OmitTopLevel(&["uri", "query"])),
        ToolVisibility::ModelVisible, TOOL_CATEGORY_PROJECT, None, TOOL_PROVIDER_CONTROL,
        ToolSemanticContract { effect: ToolEffect::Observe, risk: Read, approval: ToolApprovalPolicy::None, idempotency: ToolIdempotency::PureRead },
        None, false, NoPath, false, false, ToolSessionEvidencePolicy::NONE,
    ), &[crate::metadata::PROJECT_READ, crate::metadata::COMMUNICATION_READ]).with_activity(ToolActivityPresentation::Support, ToolActivityInteraction::NonMeaningful), description), rank, ToolDirectReason::CoreWorkflow)
}

// Authorization is selected by resource kind inside the canonical service. A common
// project/communication/session scope here would incorrectly disable other domains.
pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    resource_tool("open_webcodex_workbench", "Open the readonly Projects & Resources workbench. Empty arguments show authorized Projects for explicit selection; optional project and session_id select exact context. Supports normal tool fallback when MCP Apps are unavailable; never sends a message or starts work.", 152),
    resource_tool("search_webcodex_resources", "Discover authorized project, file, Goal or artifact resource links. File search requires project; artifact search requires project and an explicitly selected session_id. Literal queries are filtered before paging. Reports incomplete sources; references grant no authority.", 154),
    resource_tool("read_webcodex_resource", "Read the latest bounded content or metadata of one exact webcodex-resource:// reference. Re-authorizes the original domain; pinned project roots, deleted paths and inaccessible Goals fail closed. Artifacts reuse file identity; historical observations are not current facts.", 153),
];

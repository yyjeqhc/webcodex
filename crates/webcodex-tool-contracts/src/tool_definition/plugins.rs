use super::ToolVisibility::ModelVisible;
use super::{
    adaptive_runtime_direct, def, model_spec, require_any_scopes, ToolDefinition,
    TOOL_CATEGORY_PLUGIN,
};
use crate::metadata::{
    ToolPathHint::None as NoPath, ToolRisk::RunControl, PLUGIN_INSPECT, PLUGIN_INVOKE,
    PLUGIN_MANAGE, TOOL_PROVIDER_CONTROL,
};

const PLUGIN_GATEWAY_SCOPES: &[&str] = &[PLUGIN_INSPECT, PLUGIN_INVOKE, PLUGIN_MANAGE];

pub(super) const DEFINITIONS: &[ToolDefinition] = &[adaptive_runtime_direct(
    require_any_scopes(
        model_spec(
            def(
                "plugin_tool",
                super::ToolAuditPolicy::TYPED_CANONICAL,
                ModelVisible,
                TOOL_CATEGORY_PLUGIN,
                None,
                TOOL_PROVIDER_CONTROL,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Execute,
                    risk: RunControl,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::NonIdempotent,
                },
                None,
                false,
                NoPath,
                false,
                false,
                super::ToolSessionEvidencePolicy::NONE,
            ),
            "Stable gateway for Runner-owned native Tool Plugins. Provider tools are never outer WebCodex MCP tools. Discovery starts at an exact visible Runner. Startup catalog is selection metadata, not an input schema or binding. Describe the selected tool when missing, then reuse both while retained for the exact Runner/provider/tool and bound Project; another call/task/Session alone needs no describe. projectBound requires project on describe and write authority at call. Never reuse a projectBound binding for another Project. Re-describe after context loss or explicit stale/replaced/schema-change recovery; this is observation, not permission to replay an effect. Unknown outcomes still require reconciliation, never blind retry. Provider call accepts only binding + arguments and never retargets. Each action enforces plugin:inspect, plugin:invoke, or plugin:manage before dispatch.",
        ),
        PLUGIN_GATEWAY_SCOPES,
    ),
    26,
    super::ToolDirectReason::CoreWorkflow,
)];

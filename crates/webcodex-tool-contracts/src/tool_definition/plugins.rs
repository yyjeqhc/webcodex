use super::ToolVisibility::ModelVisible;
use super::{
    adaptive_runtime_direct, def, model_spec, require_any_scopes, ToolDefinition,
    TOOL_CATEGORY_RUNTIME,
};
use crate::metadata::{
    ToolPathHint::None as NoPath, ToolRisk::RunControl, PLUGIN_INSPECT, PLUGIN_INVOKE,
    PLUGIN_MANAGE, PLUGIN_MUTATE, TOOL_PROVIDER_CONTROL,
};

const PLUGIN_GATEWAY_SCOPES: &[&str] =
    &[PLUGIN_INSPECT, PLUGIN_INVOKE, PLUGIN_MUTATE, PLUGIN_MANAGE];

pub(super) const DEFINITIONS: &[ToolDefinition] = &[adaptive_runtime_direct(
    require_any_scopes(
        model_spec(
            def(
                "plugin_tool",
                super::ToolAuditPolicy::TYPED_CANONICAL,
                ModelVisible,
                TOOL_CATEGORY_RUNTIME,
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
            "Stable gateway for Runner-owned native Tool Plugins. Provider tools are never outer WebPi MCP tools. Discovery begins at an exact caller-visible Runner; describe observes one exact Runner/provider/tool schema and returns an opaque binding; call accepts only binding + arguments, never retargets, relists, reloads, or blindly retries. Gateway visibility requires any Plugin scope. list/describe require plugin:inspect; explicit read-only calls require plugin:invoke; destructive or annotation-ambiguous calls require plugin:invoke + plugin:mutate; check/reload require plugin:manage. All action-specific gates run before provider dispatch.",
        ).with_gpt_action_description("Access Runner-owned Tool Plugins. List/describe before call. Calls use an opaque binding and never retarget or retry. Read-only calls require plugin:invoke; destructive or ambiguous calls also require plugin:mutate. Check/reload require plugin:manage. Scope gates run before provider dispatch."),
        PLUGIN_GATEWAY_SCOPES,
    ),
    26,
)];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_gateway_visibility_and_description_cover_mutating_call_scope() {
        assert_eq!(
            PLUGIN_GATEWAY_SCOPES,
            [PLUGIN_INSPECT, PLUGIN_INVOKE, PLUGIN_MUTATE, PLUGIN_MANAGE]
        );
        let description = DEFINITIONS[0]
            .model_spec
            .expect("plugin_tool must be model-visible")
            .description;
        assert!(description.contains("plugin:invoke + plugin:mutate"));
        assert!(description.contains("explicit read-only calls require plugin:invoke"));
        assert!(description.contains("check/reload require plugin:manage"));
        let action_description = DEFINITIONS[0]
            .gpt_action_description()
            .expect("plugin_tool GPT Action description");
        assert!(
            action_description.chars().count() <= crate::GPT_ACTION_DESCRIPTION_MAX_CHARS,
            "plugin_tool GPT Action description exceeds {} characters",
            crate::GPT_ACTION_DESCRIPTION_MAX_CHARS
        );
    }
}

//! Kernel capability and scope admission. Checks do not dispatch or record effects.
use super::{ToolCallErrorStatus, ToolCallOutcome, ToolProtocolCapabilities};
use crate::auth::scopes::OAuthToolScopePolicy;
use crate::auth::AuthContext;
use crate::tool_runtime::tool_definition::{
    runtime_tool_operator_extension_family, ToolOperatorExtensionFamily,
};

/// Shared scope projection for authenticated discovery surfaces. Operator
/// extensions always require their declared authority; ordinary non-OAuth
/// catalogs retain their established visibility behavior.
pub(crate) fn runtime_tool_scope_allows_discovery(
    auth: Option<&AuthContext>,
    tool_name: &str,
) -> bool {
    let requires_check = auth.is_some_and(AuthContext::is_oauth_token)
        || runtime_tool_operator_extension_family(tool_name).is_some()
        || matches!(
            crate::auth::scopes::oauth_scope_policy_for_runtime_tool(tool_name),
            OAuthToolScopePolicy::RequireAny(_)
        );
    !requires_check || check_runtime_tool_scope(auth, tool_name).is_ok()
}

pub(crate) fn check_runtime_tool_scope(
    auth: Option<&AuthContext>,
    tool_name: &str,
) -> Result<(), ToolCallErrorStatus> {
    let policy = crate::auth::scopes::oauth_scope_policy_for_runtime_tool(tool_name);
    let Some(auth) = auth else {
        // Preserve historical unauthenticated compatibility for unrelated
        // internal tools, but explicit Memory and administrator authority is
        // intentionally never inferred from surface presence or a missing
        // credential. This derives only from the canonical ToolDefinition
        // authority policy; it is not a tool-name registry.
        let required_explicit_scope = match policy {
            OAuthToolScopePolicy::RequireAny(scopes) => {
                // RequireAny used to be exclusive to explicit-only Plugin
                // authority. Computer consolidation also needs an OR policy for
                // control-vs-launch discovery without changing the legacy
                // unauthenticated compatibility of the underlying operations.
                if scopes
                    .iter()
                    .copied()
                    .all(crate::auth::scopes::scope_requires_explicit_unauthenticated_authority)
                {
                    return Err(ToolCallErrorStatus::InsufficientScope {
                        required_scope: None,
                        description: format!("missing any required scope: {}", scopes.join(", ")),
                    });
                }
                None
            }
            OAuthToolScopePolicy::Require(scope)
                if matches!(
                    scope,
                    crate::auth::SCOPE_MEMORY_READ
                        | crate::auth::SCOPE_MEMORY_MANAGE
                        | crate::auth::SCOPE_ADMIN
                ) =>
            {
                Some(scope)
            }
            OAuthToolScopePolicy::RequireAll(scopes) => scopes.iter().copied().find(|scope| {
                matches!(
                    *scope,
                    crate::auth::SCOPE_MEMORY_READ
                        | crate::auth::SCOPE_MEMORY_MANAGE
                        | crate::auth::SCOPE_ADMIN
                )
            }),
            _ => None,
        };
        if let Some(scope) = required_explicit_scope {
            return Err(ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(scope),
                description: format!("missing required scope: {scope}"),
            });
        }
        return Ok(());
    };

    match policy {
        OAuthToolScopePolicy::RequireAny(scopes) => {
            if scopes.iter().copied().any(|scope| auth.has_scope(scope)) {
                Ok(())
            } else {
                Err(ToolCallErrorStatus::InsufficientScope {
                    required_scope: None,
                    description: format!("missing any required scope: {}", scopes.join(", ")),
                })
            }
        }
        OAuthToolScopePolicy::Require(scope) => {
            if auth.has_scope(scope) {
                Ok(())
            } else {
                Err(ToolCallErrorStatus::InsufficientScope {
                    required_scope: Some(scope),
                    description: format!("missing required scope: {}", scope),
                })
            }
        }
        OAuthToolScopePolicy::RequireAll(scopes) => {
            if let Some(scope) = scopes.iter().copied().find(|scope| !auth.has_scope(scope)) {
                Err(ToolCallErrorStatus::InsufficientScope {
                    required_scope: Some(scope),
                    description: format!("missing required scope: {}", scope),
                })
            } else {
                Ok(())
            }
        }
        OAuthToolScopePolicy::Unknown => {
            if auth.is_bootstrap() {
                Ok(())
            } else {
                Err(ToolCallErrorStatus::InsufficientScope {
                    required_scope: None,
                    description: "runtime tool has no declared scope policy".to_string(),
                })
            }
        }
    }
}

pub(super) fn check_session_message_resolution_scope(
    auth: Option<&AuthContext>,
    requested: bool,
) -> Result<(), ToolCallErrorStatus> {
    if requested {
        // Piggyback resolution is the same business mutation as the dedicated
        // Session tool. Reuse its canonical scope policy so a caller cannot
        // acquire Session-closure authority from an unrelated main tool scope.
        check_runtime_tool_scope(auth, "resolve_session_message")
    } else {
        Ok(())
    }
}

pub(super) fn check_protocol_capabilities(
    tool_name: &str,
    auth: Option<&AuthContext>,
    capabilities: ToolProtocolCapabilities,
) -> Result<(), ToolCallOutcome> {
    let operator_extension_family = runtime_tool_operator_extension_family(tool_name);
    if matches!(
        operator_extension_family,
        Some(ToolOperatorExtensionFamily::TraceDiagnostics)
    ) && !capabilities.trace_diagnostics
    {
        return Err(ToolCallOutcome::rejected(
            ToolCallErrorStatus::InvalidArguments {
                message: "Tool trace diagnostics are available only on Stateless MCP 2026"
                    .to_string(),
            },
        ));
    }
    if tool_name == "sync_goal_plan" && !capabilities.goal_plan_app {
        return Err(ToolCallOutcome::rejected(ToolCallErrorStatus::InvalidArguments {
                message: "Goal Plan App state is available only on Stateless MCP 2026 requests with Goal Plan App capability"
                    .to_string(),
            }));
    }
    if matches!(
        tool_name,
        "get_work_result_state" | "read_work_result_activity_detail" | "send_work_result_message"
    ) && !capabilities.work_result_app
    {
        return Err(ToolCallOutcome::rejected(ToolCallErrorStatus::InvalidArguments {
                message: "Work Result App operations are available only on Stateless MCP 2026 requests with Work Result App capability"
                    .to_string(),
            }));
    }
    if tool_name == "read_app_artifact_chunk" && !capabilities.artifact_app {
        return Err(ToolCallOutcome::rejected(
            ToolCallErrorStatus::InvalidArguments {
                message: "Presentation artifact reads require the dedicated MCP App capability"
                    .to_string(),
            },
        ));
    }
    if tool_name == "read_changed_file_diff" && !capabilities.work_result_app {
        return Err(ToolCallOutcome::rejected(ToolCallErrorStatus::InvalidArguments {
                message: "Work Result App lazy diff is available only on Stateless MCP 2026 requests with Work Result App capability"
                    .to_string(),
            }));
    }
    if matches!(
        tool_name,
        "bind_agent_continuation"
            | "recover_agent_continuation_endpoint"
            | "get_agent_continuation_state"
            | "acquire_agent_continuation_wake"
            | "prepare_agent_continuation_wake"
            | "finish_agent_continuation_wake"
            | "unbind_agent_continuation"
            | "get_agent_wait_state"
    ) && !capabilities.agent_continuation_app
    {
        return Err(ToolCallOutcome::rejected(ToolCallErrorStatus::InvalidArguments {
                message: "Agent continuation App coordination is available only on Stateless MCP 2026 requests with Agent Continuation App capability"
                    .to_string(),
            }));
    }
    if matches!(
        tool_name,
        "bind_job_terminal_continuation"
            | "get_job_terminal_continuation_state"
            | "prepare_job_terminal_continuation"
            | "finish_job_terminal_continuation"
            | "unbind_job_terminal_continuation"
    ) && !capabilities.agent_continuation_app
    {
        return Err(ToolCallOutcome::rejected(ToolCallErrorStatus::InvalidArguments {
                message: "Job terminal continuation App coordination is available only on Stateless MCP 2026 requests with MCP App continuation capability"
                    .to_string(),
            }));
    }
    // Project Memory tools are kernel-known but globally model-hidden. One
    // explicit protocol capability gates all six fixed tools; their
    // canonical ToolDefinition authority decides caller access below.
    if matches!(
        operator_extension_family,
        Some(
            ToolOperatorExtensionFamily::MemoryRuntime
                | ToolOperatorExtensionFamily::MemoryManagement
        )
    ) && !capabilities.memory_surface
    {
        return Err(ToolCallOutcome::rejected(
            ToolCallErrorStatus::InvalidArguments {
                message: "Memory tools are available only on Stateless MCP 2026".to_string(),
            },
        ));
    }
    // Phase-3 Skill tools are kernel-known only so ToolCall parsing stays
    // typed, but execution is gated by explicit protocol capability. A private
    // tool name from REST or legacy MCP cannot enable this runtime.
    if matches!(
        operator_extension_family,
        Some(ToolOperatorExtensionFamily::SkillRuntime)
    ) && !capabilities.skill_runtime
    {
        return Err(ToolCallOutcome::rejected(
            ToolCallErrorStatus::InvalidArguments {
                message: "Skill runtime tools are available only on Stateless MCP 2026".to_string(),
            },
        ));
    }
    if matches!(
        operator_extension_family,
        Some(ToolOperatorExtensionFamily::SkillManagement)
    ) && !capabilities.skill_management
    {
        return Err(ToolCallOutcome::rejected(
            ToolCallErrorStatus::InvalidArguments {
                message: "Skill management tools are available only on Stateless MCP 2026"
                    .to_string(),
            },
        ));
    }
    if matches!(
        operator_extension_family,
        Some(ToolOperatorExtensionFamily::SkillManagement)
    ) && !auth.is_some_and(|auth| auth.has_scope(crate::auth::SCOPE_ADMIN))
    {
        return Err(ToolCallOutcome::rejected(
            ToolCallErrorStatus::InsufficientScope {
                required_scope: Some(crate::auth::SCOPE_ADMIN),
                description: "missing required scope: admin".to_string(),
            },
        ));
    }
    Ok(())
}

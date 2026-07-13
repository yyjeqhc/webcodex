//! Permission evaluation entry point.
//!
//! Future call chain (Phase 2+ will move this fully before mutation):
//!
//! ```text
//! Tool Request → PermissionEvaluator → PermissionDecision → ToolRuntime
//! ```
//!
//! Phase 1 keeps post-exec attach timing; only the decision function is unified.

use super::model::PermissionDecision;
use super::policy::{decide_for_required_tool, EffectivePermissionConfig};
use super::risk::{classify_tool_risk, tool_requires_permission};
use super::PermissionMode;

/// Evaluates whether a tool invocation is permission-bearing and, if so, the
/// mode-specific decision. Does **not** execute tools and does **not** override
/// hard safety.
#[derive(Debug, Clone)]
pub(crate) struct PermissionEvaluator {
    config: EffectivePermissionConfig,
}

impl PermissionEvaluator {
    /// Evaluator using `WEBCODEX_PERMISSION_MODE` (default `dev_auto_approve`).
    pub(crate) fn from_env() -> Self {
        Self {
            config: EffectivePermissionConfig::from_env(),
        }
    }

    /// Evaluator fixed to a known mode (unit tests / explicit wiring).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn with_mode(mode: PermissionMode) -> Self {
        Self {
            config: EffectivePermissionConfig::with_mode(mode),
        }
    }

    /// Evaluator from an already-resolved config (including invalid mode).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn with_config(config: EffectivePermissionConfig) -> Self {
        Self { config }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn config(&self) -> &EffectivePermissionConfig {
        &self.config
    }

    /// Single decision entry for a concrete tool name + optional project.
    ///
    /// Returns `None` when the tool class does not require permission
    /// (`not_required`). Otherwise returns a [`PermissionDecision`].
    pub(crate) fn evaluate(
        &self,
        tool_name: &str,
        project: Option<&str>,
    ) -> Option<PermissionDecision> {
        if !tool_requires_permission(tool_name) {
            return None;
        }
        let risk = classify_tool_risk(tool_name);
        Some(decide_for_required_tool(
            &self.config,
            tool_name,
            project,
            risk,
        ))
    }
}

/// Compatibility wrapper around [`PermissionEvaluator::from_env`].
///
/// Call sites should prefer [`PermissionEvaluator`] when they already hold a
/// resolved config; this remains the thin env-backed helper.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn permission_decision_for_tool(
    tool_name: &str,
    project: Option<&str>,
) -> Option<PermissionDecision> {
    PermissionEvaluator::from_env().evaluate(tool_name, project)
}

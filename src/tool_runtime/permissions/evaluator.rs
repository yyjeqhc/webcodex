//! Permission evaluation entry point.
//!
//! Authoritative call chain (Phase 2):
//!
//! ```text
//! request validation / hard session+auth guards
//!   → PermissionEvaluator (once)
//!   → allow / deny gate
//!   → tool execution (on allow)
//!   → attach the same PermissionDecision to the result
//! ```
//!
//! The evaluator does **not** execute tools and does **not** override hard safety.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::model::PermissionDecision;
use super::policy::{decide_for_required_tool, EffectivePermissionConfig};
use super::risk::{classify_tool_risk, tool_requires_permission};
#[cfg(test)]
use super::PermissionMode;

/// Evaluates whether a tool invocation is permission-bearing and, if so, the
/// mode-specific decision. Does **not** execute tools and does **not** override
/// hard safety.
#[derive(Debug, Clone)]
pub(crate) struct PermissionEvaluator {
    config: EffectivePermissionConfig,
    /// Optional counter incremented on every [`Self::evaluate`] (tests).
    eval_count: Option<Arc<AtomicUsize>>,
}

impl PermissionEvaluator {
    /// Evaluator using `WEBCODEX_PERMISSION_MODE` (default `dev_auto_approve`).
    pub(crate) fn from_env() -> Self {
        Self {
            config: EffectivePermissionConfig::from_env(),
            eval_count: None,
        }
    }

    /// Evaluator fixed to a known mode (unit tests / explicit wiring).
    #[cfg(test)]
    pub(crate) fn with_mode(mode: PermissionMode) -> Self {
        Self {
            config: EffectivePermissionConfig::with_mode(mode),
            eval_count: None,
        }
    }

    /// Evaluator from an already-resolved config (including invalid mode).
    #[cfg(test)]
    pub(crate) fn with_config(config: EffectivePermissionConfig) -> Self {
        Self {
            config,
            eval_count: None,
        }
    }

    /// Attach a shared evaluation counter (tests prove single-eval).
    #[cfg(test)]
    pub(crate) fn with_eval_counter(mut self, counter: Arc<AtomicUsize>) -> Self {
        self.eval_count = Some(counter);
        self
    }

    #[cfg(test)]
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
        if let Some(counter) = self.eval_count.as_ref() {
            counter.fetch_add(1, Ordering::SeqCst);
        }
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
/// Call sites should prefer the runtime-held [`PermissionEvaluator`]; this
/// remains the thin env-backed helper for unit tests and profile helpers.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn permission_decision_for_tool(
    tool_name: &str,
    project: Option<&str>,
) -> Option<PermissionDecision> {
    PermissionEvaluator::from_env().evaluate(tool_name, project)
}

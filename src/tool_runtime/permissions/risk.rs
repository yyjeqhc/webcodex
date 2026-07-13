//! Risk classification interface for the permission decision layer.
//!
//! Labels come from tool metadata / policy (`tool_definition` /
//! `tool_policy`). This module is the permission-side facade so evaluators do
//! not reach into registry details directly. Complex argument-bound rules are
//! intentionally out of scope for Phase 1.

use super::super::tool_definition::{
    runtime_tool_permission_risk, runtime_tool_requires_permission,
};

/// Whether the tool class enters the permission-bearing set.
///
/// Read-only tools return false and do not produce a [`super::PermissionDecision`].
pub(crate) fn tool_requires_permission(tool_name: &str) -> bool {
    runtime_tool_requires_permission(tool_name)
}

/// Coarse risk label for summaries and future gates (e.g. `write`, `shell`).
///
/// Independent of permission outcome: high risk still auto-approves under
/// `dev_auto_approve` after hard safety.
pub(crate) fn classify_tool_risk(tool_name: &str) -> &'static str {
    runtime_tool_permission_risk(tool_name)
}

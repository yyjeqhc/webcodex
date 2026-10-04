//! Deterministic coding-task workflow aggregates.
//!
//! These tools reduce repetitive startup/finish calls for model-facing coding
//! loops. They only aggregate existing runtime state and never call an LLM,
//! generate prose summaries, parse validation output, or hide underlying tool
//! payloads.

use crate::tool_runtime::tool_audit::ToolCallAuditProjection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::continuation_feedback::{
    continuation_feedback_value, continuation_projection_hooks, continuation_validation_snapshot,
    not_applicable_continuation_feedback_value, ContinuationFeedbackInput,
    ContinuationToolFailureSnapshot,
};
use super::git_review_snapshot::{
    caller_fingerprint as review_caller_fingerprint, workspace_snapshot_complete_for_closeout,
    GitReviewSnapshot,
};
use super::handoff::{
    closeout_work_projection, compact_jobs, compact_review_evidence, compact_tool_failures,
    compact_validation, reconcile_closeout_evidence, review_evidence_summary_for_session,
    validation_has_cargo_test_zero_tests,
};
use super::handoff_brief::{build_handoff_brief, HandoffBriefInput};
use super::permissions::{
    authority_profile_payload, permission_summary_from_events, PermissionDecision,
};
use super::project_instructions::{ProjectInstructionFile, ProjectInstructionsSnapshot};
use super::project_resolution::ResolvedProject;
use super::runtime_info::compact_runtime_status;
use super::session_context::{
    absent_workflow_session_result, session_project_mismatch_result,
    session_retention_expired_result, workflow_session_authority_fingerprint,
    SessionProjectMismatch,
};
use super::sessions::tool_failure_summary_from_events;
use super::sessions::{self, SessionTransport, TOOL_CALL_RECORDING_SESSION_ID_FIELD};
use super::startup_brief::{
    build_startup_brief, builtin_coding_workflow_projection, startup_brief_from_output,
    StartupBriefInput, REPOSITORY_OVERVIEW_NOT_REQUESTED_REASON,
};
use super::startup_catalog::{
    bounded_extension_description, StartupExtensions, StartupPluginEntry, StartupPluginsCatalog,
};
use super::tool_catalog::model_visible_recommended_flows;
use super::tool_inputs::{CodingGuidanceProfile, SessionMode, StartupDetail};
use super::tool_result::{RecoveryKind, ToolResult};
use super::unknown_session_result;
use super::validation_events::skipped_validation_summary;
use super::window_activity::{
    ToolCallCorrelation, WorkflowSessionCorrelation, WorkflowSessionCorrelationRelation,
};
use super::{ToolCall, ToolRuntime};
use crate::auth::AuthContext;
use crate::runner_protocol::{
    ShellFileOpRequest, RUNNER_CAPABILITY_FILE_READ, RUNNER_CAPABILITY_GIT, RUNNER_CAPABILITY_SHELL,
};
use std::collections::HashSet;
use std::time::Duration;

/// Short startup probe budget for the repository overview, much tighter than
/// the standalone `read_project_overview` tool's 30s wait. An optional overview
/// failure must not block the coding task, so it fails over quickly.
pub(crate) const DEFAULT_REPOSITORY_OVERVIEW_PROBE_TIMEOUT: Duration = Duration::from_secs(6);

/// Request-local observations for the post-result context sidecar. Never persisted
/// or inferred from the public startup projection.
pub(crate) struct BootstrapContext {
    pub(crate) project: ResolvedProject,
    pub(crate) instructions: ProjectInstructionsSnapshot,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ProjectResolutionMetadata {
    pub(crate) source: String,
    pub(crate) outcome: String,
    pub(crate) resolved_project: String,
    pub(crate) registered: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) worktree: Option<ManagedWorktreeProjection>,
    #[serde(skip)]
    pub(crate) permission: Option<PermissionDecision>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ManagedWorktreeProjection {
    pub(crate) managed: bool,
    pub(crate) base_ref: String,
    pub(crate) base_sha: String,
    pub(crate) source_dirty: bool,
}

mod closeout;
mod observations;
mod project;
mod projection;
mod startup;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use projection::project_coding_agent_providers;
#[cfg(test)]
pub(crate) use projection::{
    project_work_on_project_output, project_work_on_project_output_with_correlation_for_test,
};

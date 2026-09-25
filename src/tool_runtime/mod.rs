//! Tool Runtime — unified execution layer for MCP and GPT Actions.
//!
//! Both protocol adapters call `ToolRuntime::dispatch()`.
//! No HTTP framework types here — pure Rust input/output.

pub mod activity;
mod agent_task;
mod agent_wait;
mod artifact_transfer;
mod browser_tools;
mod cargo;
mod cargo_tools;
mod changes;
#[cfg(feature = "workspace-checkpoints")]
mod checkpoint;
#[cfg(feature = "experimental-code-mode")]
mod code_mode;
#[cfg(feature = "experimental-code-mode")]
mod orchestration_host;
#[cfg(feature = "experimental-code-mode")]
pub(crate) use code_mode::is_admitted_nested_tool as code_mode_nested_tool_is_admitted;
mod coding_agent;
mod coding_task;
mod coding_task_tools;
mod communication;
mod computer_tools;
pub(crate) mod context_projection;
mod continuation_feedback;
pub(crate) mod control_sidecar;
pub(crate) mod conversation_import;
mod discovery_tools;
mod dispatch;
mod edit_tool_telemetry;
mod file_tools;
pub(crate) mod files;
mod git;
mod runner_authorization;
mod runner_config;
mod runner_instructions;
#[cfg(test)]
pub(crate) use git::{framed_clean_show_changes_test_stdout, framed_show_changes_test_block};
mod git_committed;
mod git_review;
mod git_tools;
mod goal;
mod handoff;
mod handoff_brief;
mod handoff_tools;
mod helpers;
mod hygiene;
mod hygiene_tools;
mod job_attention;
mod job_terminal_wait;
mod job_tools;
mod jobs;
pub(crate) mod kernel;
mod lsp_tools;
pub(crate) use lsp_tools::runner_local_project_id;
pub(crate) mod memory;
pub(crate) mod model_ergonomics_telemetry;
mod model_references;
pub(crate) mod observations;
mod observe_jobs;
mod patch;
mod patch_tools;
pub(crate) mod peer_collaboration;
pub(crate) mod permissions;
mod process;
mod project_resolution;
pub(crate) use project_resolution::ResolvedProject;
mod project_tools;
mod projects;
mod read_cache;
mod read_files;
mod read_revisions;
mod runtime;
mod runtime_info;
pub(crate) mod runtime_metrics;
mod script;
mod search_and_read;
mod search_project_texts;
mod semantic_navigation;
mod session_context;
pub(crate) use session_context::runtime_observation_principal;
pub(crate) use session_context::SESSION_ATTENTION_MAX_MESSAGES;
pub(crate) use window_activity::{
    ToolCallCorrelation, WindowActivityGuard, WindowLoopTransition, WorkflowSessionCorrelation,
    WorkflowSessionCorrelationRelation,
};
mod session_shell;
mod session_tools;
pub(crate) mod sessions;
mod shell;
mod shell_tools;
pub(crate) mod skills;
pub(crate) mod specialized;
pub(crate) mod startup_brief;
mod structured_execution;
mod surface;
pub(crate) use tool_audit::session_log_result_for_tool as audit_safe_result_for_tool;
mod current_window_activity;
mod validation_events;
pub(crate) mod validation_profile;
mod validation_source;
pub(crate) mod window_activity;
pub(crate) mod window_activity_projection;
pub(crate) use webcodex_core::{
    project_instructions, project_listing as file_listing, validation_evidence as validation_parser,
};
pub(crate) use webcodex_tool_contracts::{
    metadata, registry, tool_call, tool_catalog, tool_definition, tool_inputs,
};
#[cfg(test)]
pub(crate) use webcodex_tool_runtime_contracts::recorder_metadata::parse_tool_call_with_recorder_metadata;
pub(crate) use webcodex_tool_runtime_contracts::{tool_audit, tool_result};
mod work_result;
pub(crate) use window_activity::{ActiveWindowRequest, MAX_ACTIVE_REQUESTS_PER_WINDOW};

#[cfg(test)]
pub(crate) use webcodex_tool_contracts::MODEL_TOOL_DESCRIPTION_MAX_CHARS;

// Re-export the public API so `crate::tool_runtime::ToolCall` etc. still work.
#[cfg(test)]
pub use crate::apply_edits_shared::ApplyTextLineScope;
#[cfg(test)]
pub(crate) use files::MAX_PROJECT_ARTIFACT_BYTES;
pub(crate) use files::{
    validate_project_artifact_export_snapshot, ProjectArtifactExportSnapshot,
    INTERNAL_ARTIFACT_TRANSFER_CHUNK_BYTES, MAX_PROJECT_ARTIFACT_EXPORT_BYTES,
};
#[cfg(test)]
pub(crate) use permissions::{AuthorityMode, PermissionEvaluator};
#[cfg(test)]
pub(crate) use runner_authorization::required_runner_capability;
pub use runtime::ToolRuntime;
pub use runtime_info::RuntimeInfo;
#[cfg(test)]
pub(crate) use session_context::workflow_session_authority_fingerprint;
#[cfg(test)]
pub(crate) use sessions::{SessionCreateOptions, SessionGuards, SessionSummary};
#[cfg(test)]
pub use tool_definition::is_known_tool_name;
#[cfg(test)]
pub(crate) use tool_definition::{
    known_tool_names, model_hidden_tool_names, runtime_tool_category as tool_manifest_category,
    RunnerCapabilityRequirement,
};
pub use webcodex_tool_contracts::tool_call::{
    AgentWaitEventSelectorCall, AgentWaitModeCall, HostFileImportProvenance, ObserveJobsItem,
    ObserveJobsWakeOn, PluginToolCall, ProjectArtifactAction, ReadFilesItem, SearchPatternMode,
    SearchProjectTextsQuery, SearchResultMode, SshResourceToolCall, ToolCall,
};
pub(crate) use webcodex_tool_contracts::tool_call::{TOOL_CALL_PARAMS_FIELD, TOOL_CALL_TOOL_FIELD};
#[cfg(test)]
pub use webcodex_tool_contracts::tool_inputs::ApplyFileChangeInput;
#[cfg(all(test, feature = "workspace-checkpoints"))]
pub use webcodex_tool_contracts::tool_inputs::CheckpointValidationInput;
#[cfg(test)]
pub use webcodex_tool_contracts::tool_inputs::{
    ApplyFileChangeKind, ApplyTextEditInput, ApplyTextEditKind, SessionMode, StartupDetail,
};
pub use webcodex_tool_contracts::tool_inputs::{
    ExecutionPurpose, ExecutionShell, ListToolsOptions,
};
pub use webcodex_tool_contracts::ToolSpec;
pub use webcodex_tool_runtime_contracts::tool_result::ToolResult;
pub(crate) use webcodex_tool_runtime_contracts::tool_result::{
    ContinuationCarrier, ContinuationKind, ContinuationSemantics, RecoveryKind, SuggestedToolCall,
    RECOVERY_KIND_VALUES,
};

#[cfg(test)]
pub(crate) use project_resolution::ProjectResolverErrorKind;
pub(crate) use project_resolution::{runner_project_runtime_id, ProjectResolverError};
pub(crate) use registry::{
    agent_continuation_app_tool_specs, goal_plan_app_tool_specs,
    job_terminal_continuation_app_tool_specs, registered_tool_specs,
    stateless_operator_extension_tool_specs, work_result_app_tool_specs,
};
#[cfg(test)]
pub(crate) use registry::{
    memory_management_tool_specs, memory_runtime_tool_specs, skill_management_tool_specs,
};
pub(crate) use session_context::{add_session_hint, unknown_session_result};
pub(crate) use session_shell::SessionShellRegistry;
#[cfg(test)]
pub(crate) use surface::registered_tool_categories;

#[cfg(test)]
mod tests;

mod external_observations;

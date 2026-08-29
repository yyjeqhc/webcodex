//! Tool Runtime — unified execution layer for MCP and GPT Actions.
//!
//! Both protocol adapters call `ToolRuntime::dispatch()`.
//! No HTTP framework types here — pure Rust input/output.

pub mod activity;
mod agent_authorization;
mod cargo;
mod cargo_tools;
mod checkpoint;
mod coding_agent;
mod coding_task;
mod coding_task_tools;
mod communication;
mod computer_tools;
pub(crate) mod context_projection;
mod continuation_feedback;
pub(crate) mod conversation_import;
mod discovery_tools;
mod dispatch;
mod edit_tool_telemetry;
mod file_listing;
mod file_tools;
pub(crate) mod files;
mod git;
mod git_committed;
mod git_review;
mod git_tools;
mod handoff;
mod handoff_brief;
mod handoff_tools;
mod helpers;
mod hygiene;
mod hygiene_tools;
mod job_tools;
mod jobs;
pub(crate) mod kernel;
mod local_jobs;
mod lsp_tools;
pub(crate) mod memory;
pub(crate) mod metadata;
pub(crate) mod model_ergonomics_telemetry;
pub(crate) mod observations;
mod observe_jobs;
mod patch;
mod patch_tools;
pub(crate) mod permissions;
mod process;
pub(crate) mod project_instructions;
mod project_resolution;
mod project_tools;
mod projects;
mod read_files;
mod registry;
mod runtime;
mod runtime_info;
mod script;
mod search_project_texts;
mod semantic_navigation;
mod session_context;
mod session_shell;
mod session_tools;
pub(crate) mod sessions;
mod shell;
mod shell_tools;
pub(crate) mod skills;
pub(crate) mod startup_brief;
mod structured_execution;
mod surface;
mod tool_audit;
pub(crate) use tool_audit::session_log_result_for_tool as audit_safe_result_for_tool;
mod tool_call;
mod tool_catalog;
pub(crate) mod tool_definition;
mod tool_inputs;
mod tool_policy;
mod tool_result;
mod tool_spec;
mod validation_events;
pub(crate) mod validation_parser;
pub(crate) mod validation_profile;

/// Hard repository ceiling for model-facing ToolSpec and OpenAPI operation descriptions.
/// Prefer 300 characters or fewer when semantics remain complete; brevity must not
/// remove required authority, retry, continuation, uncertainty, safety, or recovery semantics.
#[cfg(test)]
pub(crate) const MODEL_TOOL_DESCRIPTION_MAX_CHARS: usize = 600;

// Re-export the public API so `crate::tool_runtime::ToolCall` etc. still work.
#[cfg(test)]
pub(crate) use agent_authorization::required_agent_capability;
#[cfg(test)]
pub(crate) use files::MAX_PROJECT_ARTIFACT_BYTES;
pub(crate) use files::{
    validate_project_artifact_export_snapshot, ProjectArtifactExportSnapshot,
    MAX_PROJECT_ARTIFACT_EXPORT_BYTES, MAX_READ_PROJECT_ARTIFACT_LENGTH,
};
pub(crate) use local_jobs::ACTIVE_JOB_STATUSES;
#[cfg(test)]
pub(crate) use local_jobs::{LocalJobKiller, LocalJobRecord, SystemJobKiller, TerminateOutcome};
pub(crate) use patch::MAX_UNIFIED_DIFF_BYTES;
pub use runtime::ToolRuntime;
pub use runtime_info::RuntimeInfo;
#[cfg(test)]
pub(crate) use session_context::workflow_session_authority_fingerprint;
pub use tool_call::{
    ObserveJobsItem, ReadFilesItem, SearchPatternMode, SearchProjectTextsQuery, SearchResultMode,
    ToolCall,
};
pub(crate) use tool_call::{
    TOOL_CALL_ARGUMENTS_FIELD, TOOL_CALL_PARAMS_FIELD, TOOL_CALL_TOOL_FIELD,
    TOOL_CALL_WRAPPER_FIELDS,
};
#[cfg(test)]
pub use tool_definition::is_known_tool_name;
#[cfg(test)]
pub(crate) use tool_definition::is_model_hidden_tool_name;
#[cfg(test)]
pub(crate) use tool_definition::{
    known_tool_names, model_hidden_tool_names, runtime_tool_category as tool_manifest_category,
    AgentCapability,
};
pub use tool_inputs::{
    default_true, ApplyFileChangeInput, ExecutionPurpose, ExecutionShell, ListToolsOptions,
    SessionMode,
};
#[cfg(test)]
pub use tool_inputs::{
    ApplyFileChangeKind, ApplyTextEditInput, ApplyTextEditKind, CheckpointValidationInput,
    StartupDetail,
};
pub use tool_result::ToolResult;
pub(crate) use tool_result::{
    RecoveryKind, RecoveryTool, RECOVERY_KIND_VALUES, RECOVERY_TOOL_VALUES,
};
pub use tool_spec::ToolSpec;

use serde_json::json;

#[cfg(test)]
pub(crate) use project_resolution::ProjectResolverErrorKind;
pub(crate) use project_resolution::{agent_project_runtime_id, ProjectResolverError};
#[cfg(test)]
pub(crate) use registry::{accepted_flattened_args_for_spec, start_coding_task_compatibility_spec};
pub(crate) use registry::{
    generic_tool_call_flattened_args_for_spec, memory_management_tool_specs,
    memory_runtime_tool_specs, registered_tool_specs, skill_management_tool_specs,
    skill_runtime_tool_specs,
};
pub(crate) use session_context::{add_session_telemetry_hint, unknown_session_result};
pub(crate) use session_shell::SessionShellRegistry;
#[cfg(test)]
pub(crate) use surface::{recommended_flows, registered_tool_categories};

pub(crate) fn tool_disabled_result(tool_name: &str, message: &'static str) -> ToolResult {
    let error_kind = format!("{tool_name}_disabled");
    ToolResult::err_with_output(
        message,
        json!({
            "code": error_kind.clone(),
            "error_kind": error_kind,
            "tool": tool_name,
            "message": message,
        }),
    )
    .with_recovery(RecoveryKind::NoAction, None)
}

pub(crate) fn tool_disabled_result_from_definition(tool_name: &str) -> Option<ToolResult> {
    tool_definition::runtime_tool_disabled_message(tool_name)
        .map(|message| tool_disabled_result(tool_name, message))
}

#[cfg(test)]
mod tests;

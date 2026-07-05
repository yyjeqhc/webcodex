//! Tool Runtime — unified execution layer for MCP and GPT Actions.
//!
//! Both protocol adapters call `ToolRuntime::dispatch()`.
//! No HTTP framework types here — pure Rust input/output.

mod agent_authorization;
mod cargo;
mod checkpoint;
mod codex;
mod coding_task;
mod dispatch;
mod file_tools;
pub(crate) mod files;
mod git;
mod git_tools;
mod handoff;
mod helpers;
mod hygiene;
mod job_tools;
mod jobs;
pub(crate) mod kernel;
mod local_jobs;
pub(crate) mod metadata;
mod patch;
mod permissions;
pub(crate) mod project_instructions;
mod project_resolution;
mod projects;
mod registry;
mod runtime;
mod runtime_info;
mod session_context;
mod session_tools;
pub(crate) mod sessions;
mod shell;
mod surface;
mod tool_audit;
mod tool_call;
mod tool_catalog;
pub(crate) mod tool_definition;
mod tool_inputs;
mod tool_policy;
mod tool_result;
mod tool_spec;
mod validation_events;
mod validation_parser;

// Re-export the public API so `crate::tool_runtime::ToolCall` etc. still work.
#[cfg(test)]
pub(crate) use agent_authorization::required_agent_capability;
#[allow(unused_imports)]
pub(crate) use local_jobs::{
    LocalJobKiller, LocalJobRecord, SystemJobKiller, TerminateOutcome, ACTIVE_JOB_STATUSES,
    ACTIVE_LOCAL_STATUSES,
};
#[allow(unused_imports)]
pub use runtime::ToolRuntime;
#[allow(unused_imports)]
pub use runtime_info::RuntimeInfo;
#[allow(unused_imports)]
pub use tool_call::ToolCall;
pub(crate) use tool_call::{
    TOOL_CALL_ARGUMENTS_FIELD, TOOL_CALL_PARAMS_FIELD, TOOL_CALL_TOOL_FIELD,
    TOOL_CALL_WRAPPER_FIELDS,
};
#[allow(unused_imports)]
pub use tool_definition::is_known_tool_name;
#[allow(unused_imports)]
pub(crate) use tool_definition::is_model_hidden_tool_name;
#[allow(unused_imports)]
pub(crate) use tool_definition::runtime_tool_category as tool_manifest_category;
#[allow(unused_imports)]
pub(crate) use tool_definition::AgentCapability;
#[allow(unused_imports)]
pub(crate) use tool_definition::{known_tool_names, model_hidden_tool_names};
#[allow(unused_imports)]
pub use tool_inputs::{
    default_true, ApplyTextEditInput, ApplyTextEditKind, CheckpointValidationInput,
    ListToolsOptions, SessionMode,
};
#[allow(unused_imports)]
pub use tool_result::ToolResult;
#[allow(unused_imports)]
pub use tool_spec::ToolSpec;

use serde_json::json;

#[allow(unused_imports)]
pub(crate) use crate::config::CodexConfig;
#[allow(unused_imports)]
pub(crate) use project_resolution::{
    agent_project_runtime_id, ProjectResolverError, ProjectResolverErrorKind,
};
pub(crate) use registry::{accepted_flattened_args_for_spec, registered_tool_specs};
#[allow(unused_imports)]
pub(crate) use session_context::{
    add_session_telemetry_hint, current_session_key, current_session_principal,
    session_guard_denied_result, unknown_session_result, ALLOW_CROSS_PROJECT_SESSION_FIELD,
};
#[cfg(test)]
pub(crate) use surface::{recommended_flows, registered_tool_categories};

pub(crate) const RUN_CODEX_DISABLED_MESSAGE: &str =
    "run_codex is currently disabled on model-facing surfaces; use run_job or external local Codex manually.";

pub(crate) fn tool_disabled_result(tool_name: &str, message: &'static str) -> ToolResult {
    ToolResult::err_with_output(
        message,
        json!({
            "code": format!("{tool_name}_disabled"),
            "tool": tool_name,
            "message": message,
        }),
    )
}

pub(crate) fn tool_disabled_result_from_definition(tool_name: &str) -> Option<ToolResult> {
    tool_definition::runtime_tool_disabled_message(tool_name)
        .map(|message| tool_disabled_result(tool_name, message))
}

#[cfg(test)]
mod tests;

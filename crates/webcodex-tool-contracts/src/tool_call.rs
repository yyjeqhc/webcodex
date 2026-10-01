//! Runtime tool call wire/data model and behavioral helpers.
//!
//! This module owns the model-visible tool call enum, parsing by runtime tool
//! name, and the project/session accessors used by dispatch guards and audit
//! logging.

#[cfg(feature = "workspace-checkpoints")]
use super::tool_inputs::CheckpointValidationInput;
use super::tool_inputs::{
    default_true, deserialize_optional_coding_guidance_profile, ApplyFileChangeInput,
    CodingGuidanceProfile, ExecutionPurpose, ExecutionShell, GoalLifecycleInput,
    SessionLifecycleInput, SessionMode, WorkOnProjectMode,
};
use crate::{lookup_tool_definition, model_visible_tool_names_csv};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use webcodex_core::apply_patch_shared::ApplyPatchMatchingMode;
use webcodex_core::job_observation::{
    ObservationRefRegistry, MAX_JOB_OBSERVATION_TOKEN_LEN, MAX_OBSERVATION_REF_LEN,
};
use webcodex_core::lsp_bridge::{
    CallHierarchyDirection, DEFAULT_CALL_HIERARCHY_DEPTH, DEFAULT_CALL_HIERARCHY_LIMIT,
};
use webcodex_core::plugin::{
    validate_json_value as validate_plugin_json_value,
    validate_provider_id as validate_plugin_provider_id,
    validate_tool_name as validate_plugin_tool_name, PLUGIN_MAX_ARGUMENT_BYTES,
};
use webcodex_core::runner_protocol::{
    normalize_cargo_packages, ShellScriptLanguage, CARGO_PACKAGE_MAX_ITEMS, CARGO_VALUE_MAX_BYTES,
};
use webcodex_core::runtime_contract::{
    validate_project_op_path, DEFAULT_OBSERVE_JOBS_TAIL_LINES,
    GIT_DIFF_HUNKS_CONTINUATION_MAX_BYTES,
};
use webcodex_core::workflow_session_contract::{
    strip_tool_call_expectation_metadata, validate_model_facing_assertion_name,
    validate_model_facing_result_expectation, SessionExecutionContext, SessionMessageKind,
    SessionMessagePriority, SessionMessageStatus,
};

pub const TOOL_CALL_TOOL_FIELD: &str = "tool";
pub const TOOL_CALL_PARAMS_FIELD: &str = "params";
pub const TOOL_CALL_WRAPPER_FIELDS: &[&str] = &[TOOL_CALL_TOOL_FIELD, TOOL_CALL_PARAMS_FIELD];

/// Compact model-facing form of one exact Agent continuation tuple.
/// The server stores the mapping. This text is not a credential.
pub const AGENT_CONTINUATION_REF_PATTERN: &str = "^~ac[1-9][0-9]{0,18}$";

include!("tool_call/input_types.rs");
include!("tool_call/canonical_enum.rs");
include!("tool_call/parsing.rs");

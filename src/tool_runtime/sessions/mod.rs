//! Runtime session store: model, events, queries, and JSON ledger persistence.
//!
//! External callers should continue to use `crate::tool_runtime::sessions::{...}`.
//!
//! Module layout:
//! - `store` — create/lifecycle, event recording, guards, persistence
//! - `messages` — message board post/list/resolve and discussion aggregates
//! - `model` / `events` / `query` / `persistence` / `util` — supporting pieces

mod assignment;
mod console;
mod events;
mod messages;
mod model;
mod persistence;
mod query;
mod store;
mod util;

#[cfg(test)]
mod assignment_tests;
#[cfg(test)]
mod collaboration_tests;
#[cfg(test)]
mod message_mutation_tests;
#[cfg(test)]
mod tests;

// Re-exports keep `crate::tool_runtime::sessions::{...}` stable for callers.
// Only symbols referenced outside this module are re-exported here; internal
// helpers stay `pub(super)` / module-private.
pub(crate) use console::{
    aggregate_console_list, WorkflowSessionConsoleAggregate,
    WorkflowSessionConsoleAttentionOverview, WorkflowSessionConsoleDetail,
    WorkflowSessionConsoleList, WorkflowSessionConsoleListItem,
};
pub(crate) use events::{
    canonical_tool_call_finished_events, current_attempt_event_view, exploration_tool_kind,
    is_tool_call_expectation_metadata_field, is_valid_session_id, normalize_observed_project_path,
    safe_model_facing_assertion_name, strip_tool_call_expectation_metadata,
    tool_failure_summary_from_events, tool_supports_model_facing_assertion_name,
    validate_model_facing_assertion_name,
    validation_output_summary_for_tool_result as execution_output_summary_for_tool_result,
    ExplorationToolKind, EXPLORATION_TOOL_NAMES,
};
pub(crate) use model::{
    CodingSessionError, CodingSessionRequest, CompleteSessionMessageInput,
    ListSessionMessagesFilter, PostSessionMessageInput, RecordedModelFacingToolCall,
    ReplaceSessionMessageInput, SessionAckObservation, SessionCloseError,
    SessionContextRevisionAck, SessionCreateOptions, SessionDiscussionCounts,
    SessionDiscussionSummary, SessionEvent, SessionExecutionContext,
    SessionExecutionContextUpdateError, SessionGuardDenial, SessionGuards, SessionLifecycle,
    SessionLifecycleDenial, SessionMessage, SessionMessageError, SessionMessageKind,
    SessionMessageObservationError, SessionMessagePriority, SessionMessageStatus, SessionSummary,
    SessionTransport, ToolCallRecorderMetadata, ToolCallSessionMessageResolution, ToolCallStart,
    DEFAULT_MAX_EVENTS_PER_SESSION, DEFAULT_MAX_SESSIONS, MAX_CODING_INSTRUCTION_CHARS,
    MAX_MESSAGE_COMPLETION_KEY_CHARS, MAX_MESSAGE_LIST_LIMIT, MAX_MESSAGE_RESOLUTION_CHARS,
    MAX_MODEL_VALIDATION_ASSERTION_NAME_CHARS, MAX_TOOL_CALL_ACK_MESSAGE_IDS,
    SESSION_INBOX_HIGH_GUIDANCE_ATTENTION_INSTRUCTION,
    SESSION_INBOX_HIGH_GUIDANCE_ATTENTION_REASON, TOOL_ASSERTION_NAME_FIELD,
    TOOL_CALL_ACK_SESSION_CONTEXT_REVISION_FIELD,
    TOOL_CALL_ACK_SESSION_CONTEXT_REVISION_INTERNAL_FIELD, TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD,
    TOOL_CALL_ACK_SESSION_MESSAGE_IDS_INTERNAL_FIELD, TOOL_CALL_RECORDING_SESSION_ID_FIELD,
    TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD,
    TOOL_CALL_SESSION_MESSAGE_RESOLUTION_INTERNAL_FIELD,
    TOOL_EXPECTATION_RESULT_UNEXPECTED_FAILURE,
};
pub(crate) use store::SessionStore;
pub(crate) use util::redact_and_bound_instruction;

/// Synthetic authority marker used only by the cfg(test) SessionStore convenience
/// constructors. Real runtime creation paths never write or accept this marker.
#[cfg(test)]
pub(crate) const TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT: &str =
    "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

// Test-only surface: keep the runtime re-export list narrow while still
// allowing crate-level tests to reach these constants without pub-ing `model`.
#[cfg(test)]
pub(crate) use events::session_input_summary_for_tool;
#[cfg(test)]
pub(crate) use model::{MAX_VALIDATION_EXCERPT_CHARS, TOOL_CALL_EXPECTATION_METADATA_FIELDS};

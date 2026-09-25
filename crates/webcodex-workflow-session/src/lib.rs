//! Protocol-neutral Workflow Session domain model, ledger/store, collaboration, and deterministic projections.

mod assignment;
mod audit;
mod closeout;
mod console;
mod continuation;
#[cfg(test)]
mod continuation_tests;
mod events;
mod handoff_brief;
#[cfg(test)]
mod handoff_brief_tests;
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
mod session_context_tests;
#[cfg(test)]
mod session_lifecycle_tests;
#[cfg(test)]
mod session_store_tests;

pub use closeout::closeout_work_projection;
pub use console::{
    aggregate_console_list, ConsoleValidationHooks, WorkflowSessionConsoleAggregate,
    WorkflowSessionConsoleAttentionOverview, WorkflowSessionConsoleDetail,
    WorkflowSessionConsoleList, WorkflowSessionConsoleListItem,
};
pub use continuation::{
    continuation_feedback_value, not_applicable_continuation_feedback_value,
    validation_delta_value, ContinuationFeedbackInput, ContinuationProjectionHooks,
    ContinuationToolFailureSnapshot, ContinuationValidationSnapshot, EXPLORATION_CONTINUITY_ACTION,
};
pub use events::{
    canonical_tool_call_finished_events, current_attempt_event_view, exploration_tool_kind,
    is_tool_call_expectation_metadata_field, is_valid_session_id, normalize_observed_project_path,
    public_result_expectation_satisfied, safe_model_facing_assertion_name,
    strip_tool_call_expectation_metadata, tool_failure_summary_from_events,
    tool_supports_model_facing_assertion_name, tool_supports_model_facing_result_expectation,
    validate_model_facing_assertion_name, validate_model_facing_result_expectation,
    validation_output_summary_for_tool_result as execution_output_summary_for_tool_result,
    ExplorationToolKind, SessionPathHint, SessionToolContract,
};
pub use handoff_brief::{
    build_handoff_brief, handoff_brief_size, HandoffBriefInput, HANDOFF_BRIEF_HARD_MAX_BYTES,
    HANDOFF_CHANGED_PATHS_MAX_ITEMS, HANDOFF_INSTRUCTION_MAX_CHARS, HANDOFF_NEXT_ACTIONS_MAX_ITEMS,
    HANDOFF_OPEN_FAILURES_MAX_ITEMS, HANDOFF_RECENT_FILES_MAX_ITEMS,
};
pub use model::{
    CodingSessionError, CodingSessionRequest, CompleteSessionMessageInput,
    ListSessionMessagesFilter, PostSessionMessageInput, ReplaceSessionMessageInput,
    SessionAckObservation, SessionCloseError, SessionCreateOptions, SessionDiscussionCounts,
    SessionDiscussionSummary, SessionEvent, SessionExecutionContext,
    SessionExecutionContextUpdateError, SessionGuardDenial, SessionGuards, SessionLifecycle,
    SessionLifecycleDenial, SessionMessage, SessionMessageDelivery, SessionMessageDeliveryOutcome,
    SessionMessageDeliveryReplay, SessionMessageError, SessionMessageKind,
    SessionMessageObservationError, SessionMessagePriority, SessionMessageStatus, SessionSummary,
    SessionTransport, ToolCallExpectation, ToolCallRecorderMetadata,
    ToolCallSessionMessageResolution, ToolCallStart, DEFAULT_MAX_EVENTS_PER_SESSION,
    DEFAULT_MAX_SESSIONS, MAX_CODING_INSTRUCTION_CHARS, MAX_MESSAGE_CHARS,
    MAX_MESSAGE_COMPLETION_KEY_CHARS, MAX_MESSAGE_DELIVERY_KEY_CHARS, MAX_MESSAGE_LIST_LIMIT,
    MAX_MESSAGE_RESOLUTION_CHARS, MAX_MESSAGE_TAGS, MAX_MESSAGE_TAG_CHARS,
    MAX_MODEL_VALIDATION_ASSERTION_NAME_CHARS, MAX_TOOL_CALL_ACK_MESSAGE_IDS,
    MAX_TOOL_CALL_ACK_REF_CHARS, SESSION_INBOX_ACK_REQUIRED_ATTENTION_INSTRUCTION,
    SESSION_INBOX_ACK_REQUIRED_ATTENTION_REASON, TOOL_ACCEPTED_EXIT_CODES_FIELD,
    TOOL_ASSERTION_NAME_FIELD, TOOL_CALL_ACK_REF_FIELD, TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD,
    TOOL_CALL_RECORDING_SESSION_ID_FIELD, TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD,
    TOOL_EXPECTATION_RESULT_UNEXPECTED_FAILURE, TOOL_RESULT_EXPECTATION_FIELD,
};
pub use store::SessionStore;
pub use util::redact_and_bound_instruction;

#[cfg(any(test, feature = "root-test-support"))]
pub const TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT: &str =
    "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
#[cfg(test)]
pub use events::session_input_summary_for_tool;
#[cfg(test)]
pub use model::MAX_VALIDATION_EXCERPT_CHARS;
#[cfg(test)]
pub use webcodex_core::workflow_session_contract::TOOL_CALL_EXPECTATION_METADATA_FIELDS;

#[cfg(feature = "root-test-support")]
pub mod root_test_support {
    pub use crate::events::{
        changed_paths_for_tool, observed_paths_for_successful_result,
        session_input_summary_for_tool,
    };
    pub use crate::model::{
        PersistedSessionLedger, MAX_OBSERVED_PATHS_PER_EVENT, MAX_VALIDATION_EXCERPT_CHARS,
        MESSAGE_ID_PREFIX, SESSION_LEDGER_VERSION,
    };
    pub use crate::persistence::write_ledger_atomic;
    pub use webcodex_core::workflow_session_contract::TOOL_CALL_EXPECTATION_METADATA_FIELDS;
}

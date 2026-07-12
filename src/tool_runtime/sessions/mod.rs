//! Runtime session store: model, events, queries, and JSON ledger persistence.
//!
//! External callers should continue to use `crate::tool_runtime::sessions::{...}`.

mod events;
mod model;
mod persistence;
mod query;
mod store;
mod util;

#[cfg(test)]
mod tests;

// Re-exports keep `crate::tool_runtime::sessions::{...}` stable for callers.
// `unused_imports` is expected: many items are only used outside this module.
#[allow(unused_imports)]
pub(crate) use events::{
    changed_paths_for_tool, extract_project, is_valid_session_id,
    strip_tool_call_expectation_metadata, tool_call_expectation_from_arguments,
    tool_failure_summary_from_events, SessionToolClassification,
};
#[allow(unused_imports)]
pub(crate) use model::{
    CurrentSessionKey, ListSessionMessagesFilter, PostSessionMessageInput, SessionCounts,
    SessionCreateOptions, SessionDiscussionCounts, SessionDiscussionSummary, SessionEvent,
    SessionGuardDenial, SessionGuards, SessionInboxHint, SessionInboxOpenCounts, SessionMessage,
    SessionMessageError, SessionMessageKind, SessionMessagePriority, SessionMessageStatus,
    SessionMessagesSummary, SessionStoreStatus, SessionSummary, SessionTransport,
    ToolCallExpectation, ToolCallRecorderMetadata, ToolCallStart, DEFAULT_MAX_EVENTS_PER_SESSION,
    DEFAULT_MAX_MESSAGES_PER_SESSION, DEFAULT_MAX_SESSIONS, DEFAULT_MESSAGE_LIST_LIMIT,
    MAX_MESSAGE_CHARS, MAX_MESSAGE_LIST_LIMIT, MAX_MESSAGE_RESOLUTION_CHARS, MAX_MESSAGE_TAGS,
    MAX_MESSAGE_TAG_CHARS, MAX_VALIDATION_EXCERPT_CHARS, MESSAGE_ID_PREFIX, SESSION_ID_PREFIX,
    TOOL_ASSERTION_NAME_FIELD, TOOL_CALL_EXPECTATION_METADATA_FIELDS,
    TOOL_CALL_RECORDING_SESSION_ID_FIELD, TOOL_EXPECTATION_RESULT_MATCHED,
    TOOL_EXPECTATION_RESULT_MISMATCH, TOOL_EXPECTATION_RESULT_NONE,
    TOOL_EXPECTATION_RESULT_UNEXPECTED_FAILURE, TOOL_EXPECTATION_RESULT_UNEXPECTED_SUCCESS,
    TOOL_EXPECTED_FAILURE_FIELD, TOOL_EXPECTED_FAILURE_KIND_FIELD,
    TOOL_EXPECT_FAILURE_KIND_ALIAS_FIELD,
};
pub(crate) use store::SessionStore;

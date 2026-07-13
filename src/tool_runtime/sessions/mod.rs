//! Runtime session store: model, events, queries, and JSON ledger persistence.
//!
//! External callers should continue to use `crate::tool_runtime::sessions::{...}`.
//!
//! Module layout:
//! - `store` — create/lifecycle, event recording, guards, persistence
//! - `bindings` — current-session bind/unbind/lookup
//! - `messages` — message board post/list/resolve and discussion aggregates
//! - `model` / `events` / `query` / `persistence` / `util` — supporting pieces

mod bindings;
mod events;
mod messages;
mod model;
mod persistence;
mod query;
mod store;
mod util;

#[cfg(test)]
mod tests;

// Re-exports keep `crate::tool_runtime::sessions::{...}` stable for callers.
// Only symbols referenced outside this module are re-exported here; internal
// helpers stay `pub(super)` / module-private.
pub(crate) use events::{strip_tool_call_expectation_metadata, tool_failure_summary_from_events};
pub(crate) use model::{
    CurrentSessionKey, ListSessionMessagesFilter, PostSessionMessageInput, SessionCreateOptions,
    SessionDiscussionCounts, SessionDiscussionSummary, SessionEvent, SessionGuardDenial,
    SessionGuards, SessionMessage, SessionMessageError, SessionMessageKind, SessionMessagePriority,
    SessionMessageStatus, SessionSummary, SessionTransport, ToolCallRecorderMetadata,
    DEFAULT_MAX_EVENTS_PER_SESSION, DEFAULT_MAX_SESSIONS, TOOL_ASSERTION_NAME_FIELD,
    TOOL_CALL_RECORDING_SESSION_ID_FIELD, TOOL_EXPECTATION_RESULT_UNEXPECTED_FAILURE,
    TOOL_EXPECTED_FAILURE_FIELD, TOOL_EXPECTED_FAILURE_KIND_FIELD,
    TOOL_EXPECT_FAILURE_KIND_ALIAS_FIELD,
};
pub(crate) use store::SessionStore;

// Test-only surface: keep the runtime re-export list narrow while still
// allowing crate-level tests to reach these constants without pub-ing `model`.
#[cfg(test)]
pub(crate) use model::{MAX_VALIDATION_EXCERPT_CHARS, TOOL_CALL_EXPECTATION_METADATA_FIELDS};

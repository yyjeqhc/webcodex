use super::super::input_schemas::{
    close_session_input_schema, complete_session_message_input_schema,
    current_session_input_schema, list_session_messages_input_schema,
    post_session_message_input_schema, resolve_session_message_input_schema,
    session_discussion_summary_input_schema, session_handoff_summary_input_schema,
    session_summary_input_schema, update_session_context_input_schema,
    validation_summary_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "session_summary",
            "Return a bounded structured summary from the session ledger for an explicit session_id: recorded events, message-board summary, task mode, guards, and lifecycle. Uses durable ledger data where session persistence is configured; does not rely on current-session binding.",
            session_summary_input_schema(),
        ),
        tool_spec(
            "update_session_context",
            "Update Session defaults. Requires an authorized project matching the exact Session project; cross-project escape is not supported. Context and event commit under the store lock; the background writer persists, so success does not mean disk flush. Never falls back and never creates unknown Sessions.",
            update_session_context_input_schema(),
        ),
        tool_spec(
            "close_session",
            "Explicitly close a workflow session (Active to Closed) for a required session_id. Query remains available; write/shell/mutation tools are denied. Idempotent when already closed. Never uses current-session; unknown ids fail without create. finish_coding_task does not close.",
            close_session_input_schema(),
        ),
        tool_spec(
            "validation_summary",
            "Read bounded structured validation evidence already recorded in an explicit project-scoped session ledger. Does not run Cargo or shell commands, enqueue an agent request, read project files, mutate the workspace, or replace finish_coding_task.",
            validation_summary_input_schema(),
        ),
        tool_spec(
            "post_session_message",
            "Create an ordinary bounded collaboration message such as todo, question, progress, guidance, risk, or decision. Use complete_session_message instead when a worker finishes an exact todo and must atomically answer+resolve it.",
            post_session_message_input_schema(),
        ),
        tool_spec(
            "list_session_messages",
            "Read bounded session-local messages. Supports exact message_id and reply_to lookup plus kind/status filters with deterministic AND semantics; use it to fetch one assignment or its replies without relying on the recent-message window.",
            list_session_messages_input_schema(),
        ),
        tool_spec(
            "resolve_session_message",
            "Mark a session-local ledger message resolved. Idempotent when the message is already resolved; metadata-only and never modifies project files.",
            resolve_session_message_input_schema(),
        ),
        tool_spec(
            "complete_session_message",
            "Worker completion primitive for one exact open todo: atomically creates one answer reply and resolves that todo under one Session-store mutation. completion_key makes uncertain-result retries idempotent; prefer this over separate post answer + resolve calls.",
            complete_session_message_input_schema(),
        ),
        tool_spec(
            "session_discussion_summary",
            "Return a bounded structured aggregate of session-local discussion from the recorded session ledger. Does not call an LLM or generate natural-language summaries.",
            session_discussion_summary_input_schema(),
        ),
        tool_spec(
            "session_handoff_summary",
            "Read-only handoff for multi-step tasks, explicit session_id. Reads session ledger collaboration and ledger-derived validation. Diagnostics use bounded tails or safe result metadata; validation.parser.available is false if absent. Worker/coordinator read; does not depend on current-session binding.",
            session_handoff_summary_input_schema(),
        ),
        tool_spec(
            "current_session",
            "Return this exact caller/transport/stable-window/project/canonical-root binding when it targets an active matching Session. Restores the process-local cache from the hashed durable projection after restart; missing window identity never falls back to a credential.",
            current_session_input_schema(false),
        ),
        tool_spec(
            "unbind_current_session",
            "Remove the exact current-session binding for this client window, caller, transport, project, and canonical repository root from both the process-local cache and hashed durable projection. Keeps Workflow Session history intact. Idempotent and read-only.",
            current_session_input_schema(false),
        ),
    ]
}

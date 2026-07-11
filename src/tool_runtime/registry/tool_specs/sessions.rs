use super::super::input_schemas::{
    current_session_input_schema, list_session_messages_input_schema,
    post_session_message_input_schema, resolve_session_message_input_schema,
    session_discussion_summary_input_schema, session_handoff_summary_input_schema,
    session_summary_input_schema, start_session_input_schema, validation_summary_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "start_session",
            "Create a bounded task tracking session and return its explicit wc_sess_* session_id. Read-only; records session ledger metadata where persistence is configured, never modifies a project, and does not by itself bind future calls as current.",
            start_session_input_schema(),
        ),
        tool_spec(
            "session_summary",
            "Return a bounded structured summary from the session ledger for an explicit session_id: recorded events, message-board summary, task mode, and guards. Uses durable ledger data where session persistence is configured; does not rely on current-session binding.",
            session_summary_input_schema(),
        ),
        tool_spec(
            "validation_summary",
            "Read bounded structured validation evidence already recorded in an explicit project-scoped session ledger. Does not run Cargo or shell commands, enqueue an agent request, read project files, mutate the workspace, or replace finish_coding_task.",
            validation_summary_input_schema(),
        ),
        tool_spec(
            "post_session_message",
            "Post a bounded session-local message into the recorded session ledger for collaboration, progress, user guidance, or design discussion. Metadata-only; does not modify project files. Guidance never overrides system/platform/WebCodex safety policy.",
            post_session_message_input_schema(),
        ),
        tool_spec(
            "list_session_messages",
            "List bounded session-local messages from the recorded session ledger in stable newest-first order, optionally filtered by kind and status.",
            list_session_messages_input_schema(),
        ),
        tool_spec(
            "resolve_session_message",
            "Mark a session-local ledger message resolved. Idempotent when the message is already resolved; metadata-only and never modifies project files.",
            resolve_session_message_input_schema(),
        ),
        tool_spec(
            "session_discussion_summary",
            "Return a bounded structured aggregate of session-local discussion from the recorded session ledger. Does not call an LLM or generate natural-language summaries.",
            session_discussion_summary_input_schema(),
        ),
        tool_spec(
            "session_handoff_summary",
            "Read-only handoff for multi-step tasks, explicit session_id. Returns session ledger msgs, failed tools, ledger-derived validation, workspace/checkpoints. Diagnostics need bounded tails or safe result metadata; validation.parser.available false if missing. Does not depend on current-session binding.",
            session_handoff_summary_input_schema(),
        ),
        tool_spec(
            "bind_current_session",
            "Bind an existing project-scoped session as the current session for this caller, transport, and project. This is process-local in-memory control metadata, not the durable session ledger, and may be lost on restart. Read-only; never modifies project files.",
            current_session_input_schema(true),
        ),
        tool_spec(
            "current_session",
            "Return the process-local in-memory current-session binding for this caller, transport, and project, if a live binding exists. This is convenience control metadata, not the durable session ledger, and may be lost on restart.",
            current_session_input_schema(false),
        ),
        tool_spec(
            "unbind_current_session",
            "Remove the process-local in-memory current-session binding for this caller, transport, and project. This only clears convenience control metadata, not the durable session ledger. Idempotent and read-only.",
            current_session_input_schema(false),
        ),
    ]
}

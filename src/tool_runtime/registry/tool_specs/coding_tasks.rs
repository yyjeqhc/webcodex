use super::super::input_schemas::{
    finish_coding_task_input_schema, start_coding_task_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "start_coding_task",
            "Deterministic coding-task startup aggregate. Requires project, creates a session, returns explicit session_id, project resolution, optional runtime/git/rules context, recommended flow, warnings, and current binding state. Never calls an LLM; bind_current defaults false.",
            start_coding_task_input_schema(),
        ),
        tool_spec(
            "finish_coding_task",
            "Deterministic coding-task finish aggregate for an explicit session_id. Returns show_changes, optional hygiene and handoff, validation-like ledger events, workspace warnings, and dirty-state signals. Never calls an LLM, emits raw stdout/stderr, or infers validation root causes.",
            finish_coding_task_input_schema(),
        ),
    ]
}

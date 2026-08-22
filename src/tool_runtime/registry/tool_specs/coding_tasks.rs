use super::super::input_schemas::{
    finish_coding_task_input_schema, start_coding_task_input_schema, work_on_project_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

/// Non-model-facing contract retained for direct/API compatibility. This is
/// intentionally not returned by `tool_specs()` or ordinary MCP discovery.
pub(super) fn start_coding_task_compatibility_spec() -> ToolSpec {
    tool_spec(
        "start_coding_task",
        "Advanced coding-session bootstrap for direct/API compatibility. Prefer work_on_project for ordinary model coding/review; use this entry for managed temporary projects, mode/guards, execution context, startup detail, exact resume_session_id, current binding, or new-session controls.",
        start_coding_task_input_schema(),
    )
}

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "work_on_project",
            "Canonical model entry for ordinary coding/review via an existing project or Runner path; Git not required. Supports exact Session continuation and returns compact workflow plus project instructions.",
            work_on_project_input_schema(),
        ),
        tool_spec(
            "finish_coding_task",
            "Return an optional deterministic evidence snapshot for model review, including workspace, validation, jobs, and recorded tool events. The result is advisory: it does not decide task completion, replace direct diff or test review, or generate the user-facing final report.",
            finish_coding_task_input_schema(),
        ),
    ]
}

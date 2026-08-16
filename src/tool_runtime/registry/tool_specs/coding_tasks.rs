use super::super::input_schemas::{
    finish_coding_task_input_schema, start_coding_task_input_schema, work_on_project_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "start_coding_task",
            "Starts/resumes coding Session evidence; returns built-in workflow guidance, project-local instructions, and startup context. Choose implementation_owner or independent_review in the task instruction; guidance grants no authority. resume_session_id selects exact resume.",
            start_coding_task_input_schema(),
        ),
        tool_spec(
            "work_on_project",
            "Bootstrap coding work. Use project + instruction, or client_id + path + instruction to reuse/register any policy-allowed existing directory; Git not required. Returns workflow/project guidance only, never authority.",
            work_on_project_input_schema(),
        ),
        tool_spec(
            "finish_coding_task",
            "Return an optional deterministic evidence snapshot for model review, including workspace, validation, jobs, and recorded tool events. The result is advisory: it does not decide task completion, replace direct diff or test review, or generate the user-facing final report.",
            finish_coding_task_input_schema(),
        ),
    ]
}

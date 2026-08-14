use super::super::input_schemas::{
    finish_coding_task_input_schema, start_coding_task_input_schema, work_on_project_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "start_coding_task",
            "Preferred coding bootstrap for normal or advanced tasks. Starts or continues project-scoped Session evidence and returns built-in workflow guidance, project-local instructions, and repository/startup context. It is not a role selector: choose implementation_owner or independent_review by explicitly naming the behavioral role in the task instruction; guidance is model guidance only and grants no authority. Use resume_session_id for exact explicit resume and new_session=true for deliberate isolation.",
            start_coding_task_input_schema(),
        ),
        tool_spec(
            "work_on_project",
            "Preferred normal coding bootstrap. Use project + instruction for an existing project or client_id + path + instruction for a Runner-owned absolute path; do not mix the forms. It bootstraps continuity and Session evidence, not a role: explicitly name implementation_owner or independent_review in the task instruction when that behavioral guidance should apply. Returns compact built-in workflow guidance and project-local instructions; guidance grants no authority. Use start_coding_task for advanced controls.",
            work_on_project_input_schema(),
        ),
        tool_spec(
            "finish_coding_task",
            "Return an optional deterministic evidence snapshot for model review, including workspace, validation, jobs, and recorded tool events. The result is advisory: it does not decide task completion, replace direct diff or test review, or generate the user-facing final report.",
            finish_coding_task_input_schema(),
        ),
    ]
}

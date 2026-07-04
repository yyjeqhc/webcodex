use super::super::input_schemas::{
    create_project_input_schema, empty_input_schema, list_tools_input_schema,
    register_project_input_schema, tool_manifest_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "list_tools",
            "List runtime tools. Full output includes schemas and may be large; use summary_only with category, features, or limit for bounded GPT Action discovery.",
            list_tools_input_schema(),
        ),
        tool_spec(
            "list_projects",
            "List agent-registered runtime projects, execution mode, and smoke-selection capabilities such as git_available and recommended_for_smoke.",
            empty_input_schema(),
        ),
        tool_spec(
            "register_project",
            "Register an existing directory as a WebCodex project on a selected agent. "
                .to_string()
                + "Mutation with side effects; constrained by agent policy. The agent validates "
                + "the path, writes projects_dir/<id>.toml atomically, and refreshes its "
                + "project list. Requires Bearer auth.",
            register_project_input_schema(),
        ),
        tool_spec(
            "create_project",
            "Create a new directory on the selected agent and register it as a WebCodex "
                .to_string()
                + "project. Mutation with side effects; constrained by agent policy. Creates "
                + "directory, optional template, optional git init, writes projects_dir/<id>.toml "
                + "atomically. Requires Bearer auth.",
            create_project_input_schema(),
        ),
        tool_spec(
            "list_agents",
            "List connected local/remote execution agents.",
            empty_input_schema(),
        ),
        tool_spec(
            "runtime_status",
            "Return a structured runtime health/observability summary (service "
                .to_string()
                + "metadata, projects config status, agent client summaries, and job counts). "
                + "Read-only; never exposes tokens, secrets, full env, or stdout/stderr.",
            empty_input_schema(),
        ),
        tool_spec(
            "tool_manifest",
            "Return a compact, bounded tool manifest with categories, accepted flattened args, risk "
                .to_string()
                + "summary, and recommended flows. Lightweight alternative to list_tools for "
                + "long tasks. Read-only; never exposes schemas, tokens, or internal paths.",
            tool_manifest_input_schema(),
        ),
    ]
}

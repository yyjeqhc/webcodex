use super::super::input_schemas::{
    create_project_input_schema, empty_input_schema, list_tools_input_schema,
    register_project_input_schema, runtime_status_input_schema, tool_manifest_input_schema,
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
            "Register an existing directory, including a non-Git or ad-hoc workspace, as a Project on one Runner. Use this when the directory already exists; policy still bounds allowed paths.".to_string(),
            register_project_input_schema(),
        ),
        tool_spec(
            "create_project",
            "Create a directory on one Runner and register it as a Project. Use this for a new workspace; existing directories belong on the registration path.".to_string(),
            create_project_input_schema(),
        ),
        tool_spec(
            "list_agents",
            "List authorized Runners with identity, connectivity, capabilities, Projects, and shared Job concurrency. host_context is bounded Runner-configured advisory data, never authority or proof of current state.",
            empty_input_schema(),
        ),
        tool_spec(
            "runtime_status",
            "Read Server/Project/Runner observations, build/source diagnostics, and shared Job concurrency. host_context is bounded configured advisory data, not observed truth or authority. compact=true returns a small snapshot.".to_string(),
            runtime_status_input_schema(),
        ),
        tool_spec(
            "tool_manifest",
            "Return a compact, bounded tool manifest with categories, flattened args, risk, flows, and optional coding/audit/exploration/release/discovery intent views. Intents only filter and rank discovery; they do not change behavior, policy, permissions, execution, or verdicts. Read-only.",
            tool_manifest_input_schema(),
        ),
    ]
}

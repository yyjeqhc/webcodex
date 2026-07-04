use serde_json::Value;

use super::common::{
    array_schema, nullable_schema, open_object_schema, permission_profile_schema, schema_type,
    wrapped_output_schema,
};

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "runtime_status" => Some(wrapped_output_schema(vec![
            ("service", schema_type("string", "Runtime service name.")),
            ("version", schema_type("string", "Runtime version.")),
            (
                "build",
                open_object_schema("Build revision metadata for the running binary."),
            ),
            ("server_time", schema_type("integer", "Server timestamp.")),
            ("pid", schema_type("integer", "Server process id.")),
            (
                "auth_enabled",
                schema_type("boolean", "Whether bearer auth is enabled."),
            ),
            (
                "configured_public_url",
                nullable_schema("string", "Configured public URL, when set."),
            ),
            (
                "projects",
                open_object_schema("Project counts split into server_static, agent_registered, and effective. Legacy configured/count/load_error fields are retained; prefer projects.effective for model-facing status."),
            ),
            (
                "agents",
                open_object_schema("Agent counts and client summaries."),
            ),
            ("jobs", open_object_schema("Runtime job counts.")),
            ("tools", open_object_schema("Runtime tool counts and names.")),
            (
                "permissions",
                permission_profile_schema("Current permission/approval profile. dev_auto_approve is the self-hosted development default and does not bypass hard safety checks."),
            ),
            (
                "quic",
                open_object_schema("QUIC transport status, when enabled."),
            ),
        ])),
        "list_projects" => Some(wrapped_output_schema(vec![
            (
                "projects",
                array_schema(open_object_schema("Project summary including capabilities.git_available, supports_cleanup_verification, and recommended_for_smoke."), "Runtime projects."),
            ),
            ("count", schema_type("integer", "Project count.")),
            (
                "recommended_for_smoke",
                array_schema(
                    schema_type("string", "Runtime project id recommended for smoke tests."),
                    "Runtime project ids whose capabilities.recommended_for_smoke is true.",
                ),
            ),
        ])),
        "list_agents" => Some(wrapped_output_schema(vec![
            (
                "agents",
                array_schema(open_object_schema("Agent summary."), "Agent summaries."),
            ),
            (
                "clients",
                array_schema(open_object_schema("Client summary."), "Client summaries."),
            ),
            ("count", schema_type("integer", "Agent/client count.")),
        ])),
        "list_tools" => Some(wrapped_output_schema(vec![
            (
                "tools",
                array_schema(
                    open_object_schema("Tool metadata or compact summary."),
                    "Runtime tool specs, or compact summaries when summary_only is true.",
                ),
            ),
            (
                "names",
                array_schema(schema_type("string", "Tool name."), "Returned tool names."),
            ),
            ("count", schema_type("integer", "Tool count.")),
            (
                "total_count",
                schema_type("integer", "Total number of visible runtime tools."),
            ),
            (
                "filtered_count",
                schema_type("integer", "Number of tools matching category/features before limit."),
            ),
            (
                "truncated",
                schema_type("boolean", "Whether limit truncated the matching tools."),
            ),
            ("hint", schema_type("string", "Focused discovery guidance.")),
            (
                "recommended_next",
                schema_type("string", "Recommended next discovery action."),
            ),
        ])),
        "tool_manifest" => Some(wrapped_output_schema(vec![
            (
                "schema_version",
                schema_type("integer", "Manifest schema version."),
            ),
            (
                "tool_count",
                schema_type("integer", "Total number of tools in the runtime."),
            ),
            (
                "count",
                schema_type("integer", "Returned compact tool count after filtering."),
            ),
            (
                "filtered_count",
                schema_type(
                    "integer",
                    "Number of tools after applying the optional category filter.",
                ),
            ),
            (
                "category",
                nullable_schema(
                    "string",
                    "Requested category filter, or null when no filter was applied.",
                ),
            ),
            (
                "categories",
                open_object_schema(
                    "Map of category name to the list of tool names in that category.",
                ),
            ),
            (
                "tools",
                array_schema(
                    open_object_schema(
                        "Compact tool entry: name, category, accepted_flattened_args, deprecated_or_unsupported_args, provider, risk, read_only, requires_project, path_hint, destructive, shell_like, oauth_scope.",
                    ),
                    "Compact tool entries without input/output schemas.",
                ),
            ),
            (
                "risk_summary",
                open_object_schema(
                    "Counts of tools grouped by risk class (read_only, project_write, job_run, etc.).",
                ),
            ),
            (
                "recommended_flows",
                array_schema(
                    open_object_schema("Recommended tool flow with name, purpose, and tools."),
                    "Short list of recommended tool flows for common tasks.",
                ),
            ),
        ])),
        _ => None,
    }
}

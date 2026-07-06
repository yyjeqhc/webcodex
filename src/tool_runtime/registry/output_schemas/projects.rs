use serde_json::Value;

use super::common::{nullable_schema, schema_type, wrapped_output_schema};

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "register_project" => Some(wrapped_output_schema(register_project_fields())),
        "create_project" => Some(wrapped_output_schema(create_project_fields())),
        _ => None,
    }
}

fn register_project_fields() -> Vec<(&'static str, Value)> {
    vec![
        (
            "id",
            schema_type(
                "string",
                "Project onboarding result metadata for registration or create-and-register responses. Runtime project id assigned after the agent registers the project. The schema does not bypass authorization, permission, allowed-root, or agent path policy and does not expose environment, token, or secret values.",
            ),
        ),
        (
            "agent_project_id",
            schema_type(
                "string",
                "Project onboarding result metadata for registration or create-and-register responses. Agent-local project id written into the project registry.",
            ),
        ),
        (
            "client_id",
            schema_type(
                "string",
                "Project onboarding result metadata for registration or create-and-register responses. Agent client id that handled the request.",
            ),
        ),
        (
            "name",
            schema_type(
                "string",
                "Project onboarding result metadata for registration or create-and-register responses. Project display name returned by the agent.",
            ),
        ),
        (
            "path",
            schema_type(
                "string",
                "Project onboarding result metadata path for the registered project directory; not file content, not a permission grant, and not a bypass of agent path policy.",
            ),
        ),
        (
            "description",
            nullable_schema(
                "string",
                "Project onboarding result metadata for registration or create-and-register responses. Optional project description returned by the agent, or null.",
            ),
        ),
        (
            "projects_config_path",
            schema_type(
                "string",
                "Project onboarding result metadata path for the agent projects.d config file; not file content and not a dump of the config body.",
            ),
        ),
        (
            "created_config",
            schema_type(
                "boolean",
                "Result outcome metadata. True when the agent created a new projects.d config file.",
            ),
        ),
        (
            "overwritten",
            schema_type(
                "boolean",
                "Result outcome metadata. True when overwrite replaced an existing projects.d config file.",
            ),
        ),
        (
            "allow_patch",
            schema_type(
                "boolean",
                "Project onboarding result metadata for registration or create-and-register responses. Patch permission flag recorded in the agent project config; this schema does not change permission behavior or allow arbitrary project writes and does not include file content.",
            ),
        ),
    ]
}

fn create_project_fields() -> Vec<(&'static str, Value)> {
    let mut fields = register_project_fields();
    fields.extend([
        (
            "created_directory",
            schema_type(
                "boolean",
                "Result outcome metadata. True when create_project created the project directory rather than using an existing empty directory.",
            ),
        ),
        (
            "template",
            schema_type(
                "string",
                "Create-project result metadata. Template name reported by the agent; does not change allowed-root, overwrite, empty-dir, or template behavior and does not include file content.",
            ),
        ),
        (
            "git_initialized",
            schema_type(
                "boolean",
                "Result outcome metadata. True when the agent completed git-init for the created project; does not change git-init behavior or authorization checks.",
            ),
        ),
    ]);
    fields
}

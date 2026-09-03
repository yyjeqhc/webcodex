use serde_json::Value;

use super::common::{
    array_schema, nullable_schema, open_object_schema, schema_type, wrapped_output_schema,
};

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "register_project" => Some(wrapped_output_schema(register_project_fields())),
        "unregister_project" => Some(wrapped_output_schema(unregister_project_fields())),
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
                "Project onboarding result metadata for registration or create-and-register responses. Runtime Project id assigned after the Runner registers the Project. The schema does not bypass authorization, permission, allowed-root, or Runner path policy and does not expose environment, token, or secret values.",
            ),
        ),
        (
            "agent_project_id",
            schema_type(
                "string",
                "Project onboarding result metadata for registration or create-and-register responses. Runner-local project id written into the project registry.",
            ),
        ),
        (
            "client_id",
            schema_type(
                "string",
                "Project onboarding result metadata for registration or create-and-register responses. Runner client id that handled the request.",
            ),
        ),
        (
            "name",
            schema_type(
                "string",
                "Project onboarding result metadata for registration or create-and-register responses. Project display name returned by the Runner.",
            ),
        ),
        (
            "path",
            schema_type(
                "string",
                "Project onboarding result metadata path for the registered Project directory; not file content, not a permission grant, and not a bypass of Runner path policy.",
            ),
        ),
        (
            "description",
            nullable_schema(
                "string",
                "Project onboarding result metadata for registration or create-and-register responses. Optional project description returned by the Runner, or null.",
            ),
        ),
        (
            "project_record_path",
            schema_type(
                "string",
                "Project onboarding result metadata path for one Runner project registration record TOML file; not file content and not the registered workspace path.",
            ),
        ),
        (
            "projects_config_path",
            schema_type(
                "string",
                "Deprecated compatibility alias of project_record_path. Project onboarding result metadata path for one Runner project registration record TOML file; not file content.",
            ),
        ),
        (
            "created_config",
            schema_type(
                "boolean",
                "Result outcome metadata. True when the Runner created a new project registration record.",
            ),
        ),
        (
            "overwritten",
            schema_type(
                "boolean",
                "Result outcome metadata. True when overwrite replaced an existing project registration record.",
            ),
        ),
        (
            "allow_patch",
            schema_type(
                "boolean",
                "Project onboarding result metadata for registration or create-and-register responses. Patch permission flag recorded in the Runner project config; this schema does not change permission behavior or allow arbitrary project writes and does not include file content.",
            ),
        ),
    ]
}

fn unregister_project_fields() -> Vec<(&'static str, Value)> {
    vec![
        (
            "operation",
            schema_type("string", "Lifecycle operation name; successful unregister responses report unregister."),
        ),
        (
            "project",
            schema_type("string", "Exact runtime project registration targeted by this lifecycle operation."),
        ),
        (
            "outcome",
            schema_type("string", "Terminal Runner lifecycle outcome such as unregistered or already_unregistered."),
        ),
        (
            "changed",
            schema_type("boolean", "True only when the Runner removed an existing registration."),
        ),
        (
            "revision",
            nullable_schema("string", "Post-operation registration revision when one exists, otherwise null."),
        ),
        (
            "active_jobs",
            schema_type("integer", "Active project Job count observed by the unregister fence; successful unregister is zero."),
        ),
        (
            "warnings",
            array_schema(
                open_object_schema("Bounded project lifecycle warning metadata."),
                "Bounded project lifecycle warnings.",
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

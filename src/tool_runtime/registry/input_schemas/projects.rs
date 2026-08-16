use serde_json::Value;

use super::common::object_schema;

pub(crate) fn register_project_input_schema() -> Value {
    object_schema(vec![
        ("client_id", "string", "Registered Runner client_id.", true),
        (
            "id",
            "string",
            "Project id (ASCII letters, digits, '-', '_'; no slash).",
            true,
        ),
        ("name", "string", "Human-readable project name.", true),
        (
            "path",
            "string",
            "Existing absolute directory path on the Runner. Git is not required.",
            true,
        ),
        (
            "description",
            "string",
            "Optional project description.",
            false,
        ),
        (
            "allow_patch",
            "boolean",
            "Allow patch operations on this project (default true).",
            false,
        ),
        (
            "overwrite",
            "boolean",
            "Overwrite an existing project config file (default false).",
            false,
        ),
    ])
}

pub(crate) fn unregister_project_input_schema() -> Value {
    object_schema(vec![
        (
            "project",
            "string",
            "Exact full runtime project id returned by list_projects (agent:<client_id>:<project_id>).",
            true,
        ),
        (
            "expected_revision",
            "string",
            "Exact sha256 revision returned by the same list_projects observation; stale revisions fail closed.",
            true,
        ),
    ])
}

pub(crate) fn create_project_input_schema() -> Value {
    object_schema(vec![
        ("client_id", "string", "Registered Runner client_id.", true),
        (
            "id",
            "string",
            "Project id (ASCII letters, digits, '-', '_'; no slash).",
            true,
        ),
        ("name", "string", "Human-readable project name.", true),
        (
            "path",
            "string",
            "Absolute directory path to create and register on the Runner.",
            true,
        ),
        (
            "description",
            "string",
            "Optional project description.",
            false,
        ),
        (
            "allow_patch",
            "boolean",
            "Allow patch operations on this project (default true).",
            false,
        ),
        (
            "template",
            "string",
            "Template: 'empty' (default) or 'basic'.",
            false,
        ),
        (
            "git_init",
            "boolean",
            "Initialize git in the new directory (default false).",
            false,
        ),
        (
            "allow_existing_empty",
            "boolean",
            "Allow registering an existing empty directory (default false).",
            false,
        ),
        (
            "overwrite",
            "boolean",
            "Overwrite an existing project config file (default false).",
            false,
        ),
    ])
}

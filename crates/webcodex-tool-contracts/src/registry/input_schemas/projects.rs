use serde_json::Value;

use super::common::object_schema;

pub fn register_project_input_schema() -> Value {
    object_schema(vec![
        ("client_id", "string", "Registered Runner client_id.", true),
        (
            "id",
            "string",
            "Project id (ASCII letters, digits, '-', '_'; no slash).",
            true,
        ),
        (
            "name",
            "string",
            "Human-readable project name, bounded to 120 UTF-8 bytes server-side.",
            true,
        ),
        (
            "path",
            "string",
            "Existing absolute directory path on the Runner. Git is not required.",
            true,
        ),
        (
            "description",
            "string",
            "Optional project description, bounded to 500 UTF-8 bytes server-side.",
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

pub fn unregister_project_input_schema() -> Value {
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

pub fn create_project_input_schema() -> Value {
    object_schema(vec![
        ("client_id", "string", "Registered Runner client_id.", true),
        (
            "id",
            "string",
            "Project id (ASCII letters, digits, '-', '_'; no slash).",
            true,
        ),
        (
            "name",
            "string",
            "Human-readable project name, bounded to 120 UTF-8 bytes server-side.",
            true,
        ),
        (
            "path",
            "string",
            "Absolute directory path to create and register on the Runner. If it already exists, it must be empty and adopt_existing_empty must be true.",
            true,
        ),
        (
            "description",
            "string",
            "Optional project registration description, bounded to 500 UTF-8 bytes server-side. The 'empty' template never creates project files from this metadata; the 'basic' template also includes it in generated README.md content.",
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
            "Template: 'empty' (default; generates no project files) or 'basic' (generates README.md and .gitignore). git_init is a separate explicit side effect.",
            false,
        ),
        (
            "git_init",
            "boolean",
            "Initialize git in the new directory (default false).",
            false,
        ),
        (
            "adopt_existing_empty",
            "boolean",
            "Adopt an already-existing empty target directory instead of requiring create_project to create it (default false). Non-empty directories are always rejected.",
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

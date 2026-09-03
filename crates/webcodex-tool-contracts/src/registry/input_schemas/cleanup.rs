use serde_json::Value;

use super::common::{object_schema, with_optional_session_id};

pub fn delete_project_files_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Runner-registered project id.", true),
        (
            "paths",
            "array",
            "Project-relative file paths to delete.",
            true,
        ),
    ]))
}

pub fn git_restore_paths_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Runner-registered project id.", true),
        (
            "paths",
            "array",
            "Project-relative tracked paths to restore.",
            true,
        ),
    ]))
}

pub fn discard_untracked_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Runner-registered project id.", true),
        (
            "paths",
            "array",
            "Project-relative untracked paths to remove.",
            true,
        ),
    ]))
}

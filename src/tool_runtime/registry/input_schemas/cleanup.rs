use serde_json::Value;

use super::common::{object_schema, with_optional_session_id};

pub(crate) fn delete_project_files_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        (
            "paths",
            "array",
            "Project-relative file paths to delete.",
            true,
        ),
    ]))
}

pub(crate) fn git_restore_paths_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        (
            "paths",
            "array",
            "Project-relative tracked paths to restore.",
            true,
        ),
    ]))
}

pub(crate) fn discard_untracked_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        (
            "paths",
            "array",
            "Project-relative untracked paths to remove.",
            true,
        ),
    ]))
}

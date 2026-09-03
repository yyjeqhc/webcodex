use serde_json::Value;

use super::common::{object_schema, with_optional_session_id};

pub fn write_project_file_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Runner-registered project id.", true),
        ("path", "string", "Project-relative file path.", true),
        ("content", "string", "UTF-8 file content (no NUL).", true),
        (
            "overwrite",
            "boolean",
            "Allow replacing an existing file (default false); true requires expected_sha256.",
            false,
        ),
        (
            "expected_sha256",
            "string",
            "Exact current-file sha256 required with overwrite=true; omit for new-file creation.",
            false,
        ),
    ]))
}

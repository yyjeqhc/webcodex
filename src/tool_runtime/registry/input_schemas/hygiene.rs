use serde_json::{json, Value};

use super::common::OPTIONAL_EXPLICIT_SESSION_ID_DESCRIPTION;

pub(crate) fn workspace_hygiene_check_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": {
                "type": "string",
                "description": "Runtime project id."
            },
            "max_findings": {
                "type": "integer",
                "minimum": 1,
                "maximum": 200,
                "description": "Maximum findings to return (default 50, clamped to 1..200)."
            },
            "include_tracked": {
                "type": "boolean",
                "description": "Also report tracked suspicious path names (default false). When false, only untracked entries and the dirty-worktree summary are reported. Never reads file contents."
            },
            "session_id": {
                "type": "string",
                "description": OPTIONAL_EXPLICIT_SESSION_ID_DESCRIPTION
            }
        },
        "required": ["project"],
        "additionalProperties": false,
    })
}

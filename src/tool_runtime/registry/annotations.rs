use serde_json::{json, Value};

use super::super::metadata::tool_metadata;

pub(crate) fn tool_annotations(name: &str) -> Value {
    let metadata = tool_metadata(name);
    let read_only = metadata.read_only;
    let destructive = metadata.destructive;
    let open_world = metadata.shell_like;
    let idempotent = metadata.read_only;
    json!({
        "readOnlyHint": read_only,
        "destructiveHint": destructive,
        "idempotentHint": idempotent,
        "openWorldHint": open_world,
    })
}

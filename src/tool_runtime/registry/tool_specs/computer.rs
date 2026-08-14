use super::super::input_schemas::{
    computer_accessibility_status_input_schema, computer_accessibility_tree_input_schema,
    computer_list_windows_input_schema, computer_snapshot_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "computer_list_windows",
            "List a bounded set of observable top-level windows on an exact Runner. surface_id values are opaque, process-local, and ephemeral. focused is exact-window focus when reliably known; active is the platform active/frontmost signal and may be application-level on macOS.",
            computer_list_windows_input_schema(),
        ),
        tool_spec(
            "computer_accessibility_status",
            "Read the exact Runner's macOS Accessibility trust status without prompting or changing system permission state.",
            computer_accessibility_status_input_schema(),
        ),
        tool_spec(
            "computer_accessibility_tree",
            "Inspect a previously listed exact macOS window as a bounded read-only Accessibility tree. element_id values are opaque and process-local; this tool never performs control actions and never falls back to AppleScript or shell automation.",
            computer_accessibility_tree_input_schema(),
        ),
        tool_spec(
            "computer_snapshot",
            "Capture one previously listed opaque window surface as a bounded complete image. The surface may become stale and never falls back to another window.",
            computer_snapshot_input_schema(),
        ),
    ]
}

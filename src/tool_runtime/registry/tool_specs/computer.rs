use super::super::input_schemas::{
    computer_accessibility_status_input_schema, computer_accessibility_tree_input_schema,
    computer_control_input_schema, computer_input_text_input_schema,
    computer_list_windows_input_schema, computer_snapshot_input_schema, empty_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "computer_list_targets",
            "List caller-visible Runner targets that advertise read-only Computer observation capabilities. Use this when client_id is unknown. Returns only minimal target identity, connection state, and Computer capability facts; no projects, policy, jobs, host details, or observation content.",
            empty_input_schema(),
        ),
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
            "Inspect an exact macOS window as a bounded read-only Accessibility tree. element_id values are ephemeral process-local handles for computer_control; this tool performs no control action and has no AppleScript or shell fallback.",
            computer_accessibility_tree_input_schema(),
        ),
        tool_spec(
            "computer_control",
            "Perform native macOS Accessibility press or focus on an exact reusable element_id. Stale or mismatched targets fail closed; no AppleScript or shell fallback. If delivery may have occurred but the response is lost, outcome is unknown; observe current UI state before retrying.",
            computer_control_input_schema(),
        ),
        tool_spec(
            "computer_input_text",
            "Write bounded text verbatim with native macOS AXValue to an exact reusable element_id. Target must already be focused, non-secure, unprotected, supported, enabled when known, and empty. No focus, paste, synthetic-key, or send fallback. If outcome is unknown, observe UI state before retrying.",
            computer_input_text_input_schema(),
        ),
        tool_spec(
            "computer_snapshot",
            "Capture one previously listed opaque window surface as a bounded complete image. The surface may become stale and never falls back to another window.",
            computer_snapshot_input_schema(),
        ),
    ]
}

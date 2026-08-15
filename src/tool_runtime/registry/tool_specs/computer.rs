use super::super::input_schemas::{
    computer_accessibility_status_input_schema, computer_accessibility_tree_input_schema,
    computer_activate_window_input_schema, computer_control_input_schema,
    computer_element_state_input_schema, computer_find_elements_input_schema,
    computer_input_text_input_schema, computer_list_windows_input_schema,
    computer_snapshot_input_schema, empty_input_schema,
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
            "computer_find_elements",
            "Find a small bounded set of semantic elements on an exact macOS window without making the model parse the full Accessibility tree. At least one role, subrole, label, focused, or enabled filter is required. Matching is deterministic and read-only; returned element_id values are fresh ephemeral handles from the same bounded observation path.",
            computer_find_elements_input_schema(),
        ),
        tool_spec(
            "computer_element_state",
            "Revalidate one exact ephemeral macOS Accessibility element and return normalized read-only affordances plus its observation generation. The tool never returns the element's true value; protected or secure elements suppress value_empty. Stale element handles must be reacquired with computer_find_elements; stale surfaces with computer_list_windows.",
            computer_element_state_input_schema(),
        ),
        tool_spec(
            "computer_activate_window",
            "Activate and raise one exact previously observed macOS window surface. The tool accepts no app name, PID, path, bundle, command, or fallback target. Stale surfaces fail closed; if delivery may have occurred but the response is lost, observe current UI state before retrying.",
            computer_activate_window_input_schema(),
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

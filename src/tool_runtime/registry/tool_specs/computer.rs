use super::super::input_schemas::{
    computer_accessibility_status_input_schema, computer_accessibility_tree_input_schema,
    computer_activate_window_input_schema, computer_control_input_schema,
    computer_element_state_input_schema, computer_find_elements_input_schema,
    computer_input_text_input_schema, computer_key_input_input_schema,
    computer_launch_application_input_schema, computer_list_applications_input_schema,
    computer_list_displays_input_schema, computer_list_windows_input_schema,
    computer_pointer_input_schema, computer_read_clipboard_input_schema,
    computer_save_snapshot_input_schema, computer_scroll_to_element_input_schema,
    computer_snapshot_display_input_schema, computer_snapshot_input_schema,
    computer_write_clipboard_input_schema, empty_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "computer_read_clipboard",
            "Read bounded plain Unicode text from the exact macOS or Windows global clipboard. Returns UTF-8 text up to 16 KiB without truncation. This separate privacy authority never enumerates formats, accesses windows/focus, pastes, retries, uses shell fallback, or reads clipboard history.",
            computer_read_clipboard_input_schema(),
        ),
        tool_spec(
            "computer_write_clipboard",
            "Replace the exact macOS or Windows global clipboard with bounded plain Unicode text, clearing prior formats first. It never pastes, focuses, activates, restores old data, retries, or reads back implicitly. Reconcile unknown outcomes only with separate clipboard-read authority.",
            computer_write_clipboard_input_schema(),
        ),
        tool_spec(
            "computer_pointer_move",
            "Move the macOS or Windows pointer to an exact display-local source coordinate using the latest unspent display snapshot generation. The generation is single-use at effect admission. No implicit observation, focus, drag, fallback, or retry; reconcile unknown outcomes with a fresh display snapshot.",
            computer_pointer_input_schema(),
        ),
        tool_spec(
            "computer_pointer_click",
            "Send a macOS or Windows left click at an exact display-local source coordinate using the latest unspent snapshot generation. Held mouse/modifier state fails closed. No other buttons, double click, drag, implicit snapshot, fallback, or retry; reconcile unknown outcomes with a fresh display snapshot.",
            computer_pointer_input_schema(),
        ),
        tool_spec(
            "computer_list_targets",
            "List caller-visible Runners that advertise Computer observation or application capabilities. Use when client_id is unknown. Returns only target identity, connection state, and independent Computer capability facts; no projects, policy, jobs, host details, or observation content.",
            empty_input_schema(),
        ),
        tool_spec(
            "computer_list_windows",
            "List a bounded set of observable top-level windows on an exact Runner. surface_id values are opaque, process-local, and ephemeral. focused is exact-window focus when reliably known; active is the platform active/frontmost signal and may be application-level on macOS.",
            computer_list_windows_input_schema(),
        ),
        tool_spec(
            "computer_list_displays",
            "List bounded fresh full displays on an exact macOS or Windows Runner. IDs are opaque, process-local, ephemeral, and replaced on each list. Returns only display-relative dimensions and primary status; native identity, global origin, scale, and topology stay private.",
            computer_list_displays_input_schema(),
        ),
        tool_spec(
            "computer_list_applications",
            "List a bounded fresh set of installed applications on an exact macOS or Windows Runner. application_id values are opaque, process-local, ephemeral, and replaced by each fresh list; application paths, bundle/Shell identities, and native launch targets are never exposed.",
            computer_list_applications_input_schema(),
        ),
        tool_spec(
            "computer_launch_application",
            "Launch one fresh application_id through its exact macOS or Windows application target. Accepts no path, argv, cwd, environment, shell, URL, fallback, focus, or activation. Success proves only bounded launch completion, not process/window readiness; reconcile unknown outcomes with fresh windows.",
            computer_launch_application_input_schema(),
        ),
        tool_spec(
            "computer_accessibility_status",
            "Read the exact Runner's native accessibility availability/trust status without prompting or changing system permission state. Supports macOS Accessibility and Windows UI Automation.",
            computer_accessibility_status_input_schema(),
        ),
        tool_spec(
            "computer_accessibility_tree",
            "Inspect an exact macOS or Windows window as a bounded read-only native accessibility tree. element_id values are ephemeral process-local handles; this tool performs no control action and has no shell or platform-script fallback.",
            computer_accessibility_tree_input_schema(),
        ),
        tool_spec(
            "computer_find_elements",
            "Find bounded semantic elements on an exact macOS or Windows window without parsing the full native accessibility tree. Requires at least one role, subrole, label, focused, or enabled filter. Matching is deterministic and read-only; returned element_id values are fresh ephemeral handles.",
            computer_find_elements_input_schema(),
        ),
        tool_spec(
            "computer_element_state",
            "Revalidate an exact ephemeral accessibility element and return normalized read-only affordances plus observation generation. Supports macOS AX and Windows UIA; never returns element values. Protected or secure targets suppress value_empty. Reacquire stale handles with computer_find_elements.",
            computer_element_state_input_schema(),
        ),
        tool_spec(
            "computer_activate_window",
            "Activate and raise one exact previously observed macOS or Windows window surface. The tool accepts no app name, PID, path, bundle, command, or fallback target. Stale surfaces fail closed; if delivery may have occurred but the response is lost, observe current UI state before retrying.",
            computer_activate_window_input_schema(),
        ),
        tool_spec(
            "computer_control",
            "Perform native macOS Accessibility or Windows UI Automation press/focus on an exact reusable element_id. Stale, protected, disabled, or unsupported targets fail closed. There is no script, shell, coordinate, or generic fallback; uncertain post-dispatch outcomes require re-observation.",
            computer_control_input_schema(),
        ),
        tool_spec(
            "computer_scroll_to_element",
            "Scroll one exact macOS Accessibility or Windows UIA element into view with the native semantic scroll action. Stale, mismatched, unsupported, or protected targets fail closed; no wheel, coordinate, script, or shell fallback. Unknown post-dispatch outcomes require re-observation before retrying.",
            computer_scroll_to_element_input_schema(),
        ),
        tool_spec(
            "computer_key_input",
            "Send one closed key to an exact revalidated frontmost/focused macOS or Windows window. Windows rejects command, unsafe system chords, and interfering held keys; input uses the shared input stream. No text, keycodes, repeat/held state, implicit focus, or fallback. Re-observe unknown outcomes.",
            computer_key_input_input_schema(),
        ),
        tool_spec(
            "computer_input_text",
            "Write bounded text via macOS AXValue or Windows UIA ValuePattern to an exact element_id. Target must be focused, non-secure, unprotected, supported, enabled, writable, and empty; Windows must be foreground. No focus/activation, paste, key, or send fallback. Re-observe unknown outcomes.",
            computer_input_text_input_schema(),
        ),
        tool_spec(
            "computer_snapshot",
            "Capture one exact listed window as a bounded image. Optional surface-relative region or max dimensions require the region-snapshot capability; whole-window capture stays rolling-compatible. Region must fit the revalidated surface. Encoding is system-selected; stale surfaces never fall back.",
            computer_snapshot_input_schema(),
        ),
        tool_spec(
            "computer_snapshot_display",
            "Capture one discovered full display on an exact macOS or Windows Runner after private native-identity revalidation. Stale or ambiguous displays fail closed; max dimensions only downscale. No global coordinates, regions, activation, input, shell, or fallback. Returns a positive snapshot generation.",
            computer_snapshot_display_input_schema(),
        ),
        tool_spec(
            "computer_save_snapshot",
            "Save one exact window snapshot as a create-only project artifact without returning image bytes. Reuses computer_snapshot region/downscale semantics and requires computer:read plus project:write. No overwrite or encoding control. Unknown writes require artifact-metadata reconciliation before retry.",
            computer_save_snapshot_input_schema(),
        ),
    ]
}

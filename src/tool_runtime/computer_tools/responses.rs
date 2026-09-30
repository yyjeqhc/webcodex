//! Success receipt selection, bound to the original request expectations.

use super::accessibility::{
    validate_accessibility_status, validate_accessibility_tree, validate_computer_element_state,
};
use super::effects::{
    computer_application_effect_outcome_unknown, computer_clipboard_write_outcome_unknown,
    computer_effect_validated_result, computer_error, computer_pointer_effect_outcome_unknown,
    ClipboardWriteContext, PointerRequestContext,
};
use super::input_receipts::{
    validate_computer_activate_window, validate_computer_control, validate_computer_input_text,
    validate_computer_key_input, validate_computer_launch_application, validate_computer_pointer,
    validate_computer_read_clipboard, validate_computer_scroll_to_element,
    validate_computer_write_clipboard,
};
use super::screens::{validate_application_list, validate_display_list, validate_window_list};
use super::snapshot_receipts::{validate_display_snapshot, validate_snapshot};
use super::*;

/// Request-local expectations bind untrusted success receipts to the dispatched operation.
pub(super) struct ComputerResponseContext<'a> {
    pub(super) client_id: &'a str,
    pub(super) kind: &'static str,
    pub(super) list_limit: Option<usize>,
    pub(super) expected_surface_id: Option<&'a str>,
    pub(super) accessibility_bounds: Option<(usize, usize)>,
    pub(super) expected_application_id: Option<String>,
    pub(super) expected_display_id: Option<String>,
    pub(super) expected_element_id: Option<String>,
    pub(super) expected_action: Option<String>,
    pub(super) expected_key: Option<String>,
    pub(super) expected_key_modifiers: Value,
    pub(super) expected_text_bytes: Option<usize>,
    pub(super) snapshot_advanced: bool,
    pub(super) expected_snapshot_region: Option<Value>,
    pub(super) expected_snapshot_max_width: Option<u64>,
    pub(super) expected_snapshot_max_height: Option<u64>,
    pub(super) pointer_context: Option<PointerRequestContext>,
    pub(super) clipboard_write_context: Option<ClipboardWriteContext>,
}

pub(super) fn validate_computer_response(
    output: Value,
    context: ComputerResponseContext<'_>,
) -> ToolResult {
    let ComputerResponseContext {
        client_id,
        kind,
        list_limit,
        expected_surface_id,
        accessibility_bounds,
        expected_application_id,
        expected_display_id,
        expected_element_id,
        expected_action,
        expected_key,
        expected_key_modifiers,
        expected_text_bytes,
        snapshot_advanced,
        expected_snapshot_region,
        expected_snapshot_max_width,
        expected_snapshot_max_height,
        pointer_context,
        clipboard_write_context,
    } = context;
    match kind {
        "computer_list_windows" => {
            validate_window_list(output, list_limit.unwrap_or(MAX_WINDOWS))
        }
        "computer_list_applications" => {
            validate_application_list(output, list_limit.unwrap_or(MAX_APPLICATIONS))
        }
        "computer_list_displays" => {
            validate_display_list(output, list_limit.unwrap_or(MAX_DISPLAYS))
        }
        "computer_read_clipboard" => validate_computer_read_clipboard(output),
        "computer_write_clipboard" => {
            let context = clipboard_write_context.as_ref().expect("clipboard write context");
            let validated = validate_computer_write_clipboard(output, context);
            if validated.success {
                validated
            } else {
                computer_clipboard_write_outcome_unknown(
                    "Runner reported successful clipboard replacement but returned inconsistent metadata",
                    context,
                    Some(true),
                )
            }
        }
        "computer_launch_application" => {
            let validated = validate_computer_launch_application(
                output,
                expected_application_id.as_deref().unwrap_or_default(),
            );
            if validated.success {
                validated
            } else {
                computer_application_effect_outcome_unknown(
                    "Runner reported successful application launch but returned inconsistent metadata",
                    client_id,
                    expected_application_id.as_deref().unwrap_or_default(),
                )
            }
        }
        "computer_snapshot" | "computer_snapshot_region" => validate_snapshot(
            output,
            expected_surface_id.unwrap_or_default(),
            client_id,
            snapshot_advanced,
            expected_snapshot_region.as_ref(),
            expected_snapshot_max_width,
            expected_snapshot_max_height,
        ),
        "computer_snapshot_display" => validate_display_snapshot(
            output,
            expected_display_id.as_deref().unwrap_or_default(),
            client_id,
            expected_snapshot_max_width,
            expected_snapshot_max_height,
        ),
        "computer_pointer_move" | "computer_pointer_click" => {
            let context = pointer_context.as_ref().expect("pointer context");
            let validated = validate_computer_pointer(output, context);
            if validated.success {
                validated
            } else {
                computer_pointer_effect_outcome_unknown(
                    "Runner reported successful pointer input but returned inconsistent metadata",
                    context,
                )
            }
        }
        "computer_accessibility_status" => validate_accessibility_status(output),
        "computer_accessibility_tree" => {
            let (max_depth, max_nodes) = accessibility_bounds.unwrap_or((0, 0));
            validate_accessibility_tree(
                output,
                expected_surface_id.unwrap_or_default(),
                max_depth,
                max_nodes,
            )
        }
        "computer_element_state" => validate_computer_element_state(
            output,
            expected_surface_id.unwrap_or_default(),
            expected_element_id.as_deref().unwrap_or_default(),
        ),
        "computer_activate_window" => computer_effect_validated_result(
            validate_computer_activate_window(
                output,
                expected_surface_id.unwrap_or_default(),
            ),
            "Runner reported successful computer window activation but returned inconsistent metadata; inspect current UI state before retrying",
        ),
        "computer_control" => computer_effect_validated_result(
            validate_computer_control(
                output,
                expected_surface_id.unwrap_or_default(),
                expected_element_id.as_deref().unwrap_or_default(),
                expected_action.as_deref().unwrap_or_default(),
            ),
            "Runner reported successful computer control but returned inconsistent metadata; inspect current UI state before retrying",
        ),
        "computer_scroll_to_element" => computer_effect_validated_result(
            validate_computer_scroll_to_element(
                output,
                expected_surface_id.unwrap_or_default(),
                expected_element_id.as_deref().unwrap_or_default(),
            ),
            "Runner reported successful computer scroll but returned inconsistent metadata; inspect current UI state before retrying",
        ),
        "computer_key_input" => computer_effect_validated_result(
            validate_computer_key_input(
                output,
                expected_surface_id.unwrap_or_default(),
                expected_key.as_deref().unwrap_or_default(),
                &expected_key_modifiers,
            ),
            "Runner reported successful computer key input but returned inconsistent metadata; inspect current UI state before retrying",
        ),
        "computer_input_text" => computer_effect_validated_result(
            validate_computer_input_text(
                output,
                expected_surface_id.unwrap_or_default(),
                expected_element_id.as_deref().unwrap_or_default(),
                expected_text_bytes.unwrap_or_default(),
            ),
            "Runner reported successful computer text input but returned inconsistent metadata; inspect current UI state before retrying",
        ),
        _ => computer_error("invalid_request", "unsupported computer request kind"),
    }
}

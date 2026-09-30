//! Delivery certainty, effect-specific diagnostics and exact recovery guidance.

use super::inputs::{valid_application_id, valid_display_id};
use super::*;
pub(super) fn computer_error_recovery_message(error_kind: &str, error: &str) -> String {
    match error_kind {
        "stale_element" => format!(
            "{error}; reacquire a fresh element_id with computer_observe(action=find_elements) on the same surface"
        ),
        "stale_surface" => format!(
            "{error}; reacquire a fresh surface_id with computer_observe(action=windows) before continuing"
        ),
        "stale_application" => format!(
            "{error}; reacquire a fresh application_id with computer_observe(action=applications) before another launch"
        ),
        "stale_display" => format!(
            "{error}; reacquire a fresh display_id with computer_observe(action=displays) before continuing"
        ),
        _ => error.to_string(),
    }
}

pub(super) fn computer_request_is_effect(kind: &str) -> bool {
    matches!(
        kind,
        "computer_activate_window"
            | "computer_control"
            | "computer_scroll_to_element"
            | "computer_key_input"
            | "computer_input_text"
            | "computer_pointer_move"
            | "computer_pointer_click"
            | "computer_write_clipboard"
            | "computer_launch_application"
    )
}

pub(super) fn computer_suggested_recovery(
    mut result: ToolResult,
    tool: &'static str,
    arguments: Value,
) -> ToolResult {
    result
        .output
        .as_object_mut()
        .expect("Computer recovery output is an object")
        .insert(
            "suggested_call".to_string(),
            SuggestedToolCall::fallback_recovery(tool, arguments).to_value(),
        );
    result
}

fn computer_observe_suggested_recovery(
    result: ToolResult,
    action: &'static str,
    mut arguments: Value,
) -> ToolResult {
    arguments
        .as_object_mut()
        .expect("Computer observe recovery arguments are an object")
        .insert("action".to_string(), json!(action));
    computer_suggested_recovery(result, "computer_observe", arguments)
}

fn computer_reconcile_recovery(
    mut result: ToolResult,
    recovery_kind: RecoveryKind,
    reconcile_with: &str,
) -> ToolResult {
    result
        .output
        .as_object_mut()
        .expect("Computer recovery output is an object")
        .insert("reconcile_with".to_string(), json!(reconcile_with));
    result.with_recovery(recovery_kind)
}

pub(super) fn computer_error_with_client(
    kind: &str,
    message: &str,
    client_id: Option<&str>,
) -> ToolResult {
    let result = ToolResult::err_with_output(
        message.to_string(),
        json!({"error_kind": kind, "message": bounded_text(message)}),
    );
    match kind {
        // The original finder filters are not retained here. Knowing only the
        // canonical gateway family is insufficient to manufacture a safe finder call.
        "stale_element" => {
            computer_reconcile_recovery(result, RecoveryKind::Reobserve, "computer_observe")
        }
        "stale_surface" => match client_id {
            Some(client_id) => computer_observe_suggested_recovery(
                result,
                "windows",
                json!({"client_id": client_id}),
            ),
            None => {
                computer_reconcile_recovery(result, RecoveryKind::Reobserve, "computer_observe")
            }
        },
        "stale_application" => match client_id {
            Some(client_id) => computer_observe_suggested_recovery(
                result,
                "applications",
                json!({"client_id": client_id}),
            ),
            None => {
                computer_reconcile_recovery(result, RecoveryKind::Reobserve, "computer_observe")
            }
        },
        "stale_display" => match client_id {
            Some(client_id) => computer_observe_suggested_recovery(
                result,
                "displays",
                json!({"client_id": client_id}),
            ),
            None => {
                computer_reconcile_recovery(result, RecoveryKind::Reobserve, "computer_observe")
            }
        },
        "invalid_request" => result.with_recovery(RecoveryKind::FixInput),
        "permission_denied" => result.with_recovery(RecoveryKind::UserAction),
        _ => result,
    }
}

pub(super) fn computer_error(kind: &str, message: &str) -> ToolResult {
    computer_error_with_client(kind, message, None)
}

#[derive(Clone, Debug)]
pub(super) struct PointerRequestContext {
    pub(super) client_id: String,
    pub(super) display_id: String,
    pub(super) snapshot_generation: u32,
    pub(super) x: u32,
    pub(super) y: u32,
}

pub(super) fn computer_pointer_effect_not_started(
    error_kind: &str,
    message: &str,
    context: &PointerRequestContext,
) -> ToolResult {
    let mut output = json!({
        "error_kind": error_kind,
        "x": context.x,
        "y": context.y,
        "state_changed": false,
        "execution_state": "not_started",
    });
    let object = output
        .as_object_mut()
        .expect("pointer not-started output is an object");
    if valid_display_id(&context.display_id) {
        object.insert("display_id".to_string(), json!(context.display_id));
    }
    if context.snapshot_generation > 0 {
        object.insert(
            "snapshot_generation".to_string(),
            json!(context.snapshot_generation),
        );
    }
    let result = ToolResult::err_with_output(message.to_string(), output);
    match error_kind {
        "stale_display" => computer_observe_suggested_recovery(
            result,
            "displays",
            json!({"client_id": context.client_id}),
        ),
        "stale_snapshot_generation" if valid_display_id(&context.display_id) => {
            computer_observe_suggested_recovery(
                result,
                "snapshot_display",
                json!({"client_id": context.client_id, "display_id": context.display_id}),
            )
        }
        "stale_snapshot_generation" => {
            computer_reconcile_recovery(result, RecoveryKind::Reobserve, "computer_observe")
        }
        "invalid_request" => result.with_recovery(RecoveryKind::FixInput),
        "permission_denied" => result.with_recovery(RecoveryKind::UserAction),
        _ => result,
    }
}

fn computer_pointer_effect_spent_not_started(
    message: &str,
    context: &PointerRequestContext,
) -> ToolResult {
    let safe_message = format!(
        "{message}; snapshot_generation is spent. Reconcile with computer_observe(action=snapshot_display) before another pointer effect"
    );
    let result = computer_pointer_effect_not_started("not_started", &safe_message, context);
    if valid_display_id(&context.display_id) {
        computer_observe_suggested_recovery(
            result,
            "snapshot_display",
            json!({"client_id": context.client_id, "display_id": context.display_id}),
        )
    } else {
        computer_reconcile_recovery(result, RecoveryKind::Reobserve, "computer_observe")
    }
}

pub(super) fn computer_pointer_effect_outcome_unknown(
    message: &str,
    context: &PointerRequestContext,
) -> ToolResult {
    let safe_message = format!(
        "{message}; do not blindly retry. Reconcile with computer_observe(action=snapshot_display) first"
    );
    let result = ToolResult::err_with_output(
        safe_message.clone(),
        json!({
            "error_kind": "outcome_unknown",
            "display_id": context.display_id,
            "snapshot_generation": context.snapshot_generation,
            "x": context.x,
            "y": context.y,
            "execution_state": "outcome_unknown",
        }),
    );
    if valid_display_id(&context.display_id) {
        computer_observe_suggested_recovery(
            result,
            "snapshot_display",
            json!({"client_id": context.client_id, "display_id": context.display_id}),
        )
    } else {
        computer_reconcile_recovery(result, RecoveryKind::Reobserve, "computer_observe")
    }
}

pub(super) fn computer_pointer_effect_delivery_failure(
    message: &str,
    request_dispatched: Option<bool>,
    context: &PointerRequestContext,
) -> ToolResult {
    if request_dispatched == Some(false) {
        computer_pointer_effect_not_started("not_started", message, context)
    } else {
        computer_pointer_effect_outcome_unknown(message, context)
    }
}

pub(super) fn computer_pointer_runner_error(
    error: &str,
    request_dispatched: Option<bool>,
    context: &PointerRequestContext,
) -> ToolResult {
    let error_kind = classify_runner_error(error);
    match error_kind {
        "outcome_unknown" => computer_pointer_effect_outcome_unknown(
            "Runner reported an uncertain native pointer outcome",
            context,
        ),
        "not_started" => computer_pointer_effect_spent_not_started(error, context),
        "pointer_input_failed"
        | "stale_snapshot_generation"
        | "stale_display"
        | "invalid_request"
        | "unsupported_platform"
        | "permission_denied" => computer_pointer_effect_not_started(error_kind, error, context),
        _ => computer_pointer_effect_delivery_failure(
            "Runner pointer effect ended without a recognized structured result",
            request_dispatched,
            context,
        ),
    }
}

#[derive(Clone, Debug)]
pub(super) struct ClipboardWriteContext {
    pub(super) text_bytes: Option<usize>,
}

fn clipboard_write_output_base(context: &ClipboardWriteContext) -> serde_json::Map<String, Value> {
    let mut output = serde_json::Map::new();
    if let Some(text_bytes) = context
        .text_bytes
        .filter(|bytes| (1..=MAX_CLIPBOARD_TEXT_BYTES).contains(bytes))
    {
        output.insert("text_bytes".to_string(), json!(text_bytes));
    }
    output
}

pub(super) fn computer_clipboard_write_not_started(
    error_kind: &str,
    message: &str,
    context: &ClipboardWriteContext,
) -> ToolResult {
    let mut output = clipboard_write_output_base(context);
    output.insert("error_kind".to_string(), json!(error_kind));
    output.insert("execution_state".to_string(), json!("not_started"));
    output.insert("state_changed".to_string(), json!(false));
    let result = ToolResult::err_with_output(message.to_string(), Value::Object(output));
    match error_kind {
        "invalid_request" => result.with_recovery(RecoveryKind::FixInput),
        "permission_denied" => result.with_recovery(RecoveryKind::UserAction),
        _ => result,
    }
}

pub(super) fn computer_clipboard_write_outcome_unknown(
    message: &str,
    context: &ClipboardWriteContext,
    state_changed: Option<bool>,
) -> ToolResult {
    let safe_message = format!(
        "{message}; do not blindly retry. If separately authorized for computer:clipboard_read, the caller may explicitly use computer_read_clipboard to reconcile current state"
    );
    let mut output = clipboard_write_output_base(context);
    output.insert("error_kind".to_string(), json!("outcome_unknown"));
    output.insert("execution_state".to_string(), json!("outcome_unknown"));
    if let Some(state_changed) = state_changed {
        output.insert("state_changed".to_string(), json!(state_changed));
    }
    ToolResult::err_with_output(safe_message, Value::Object(output))
        .with_recovery(RecoveryKind::Reobserve)
}

pub(super) fn computer_clipboard_write_delivery_failure(
    message: &str,
    request_dispatched: Option<bool>,
    context: &ClipboardWriteContext,
) -> ToolResult {
    if request_dispatched == Some(false) {
        computer_clipboard_write_not_started("not_started", message, context)
    } else {
        computer_clipboard_write_outcome_unknown(message, context, None)
    }
}

pub(super) fn computer_clipboard_write_runner_error(
    error: &str,
    request_dispatched: Option<bool>,
    context: &ClipboardWriteContext,
) -> ToolResult {
    let error_kind = classify_runner_error(error);
    match error_kind {
        "not_started" => computer_clipboard_write_not_started("not_started", error, context),
        "outcome_unknown" => computer_clipboard_write_outcome_unknown(error, context, Some(true)),
        "invalid_request" | "unsupported_platform" | "permission_denied" => {
            computer_clipboard_write_not_started(error_kind, error, context)
        }
        _ => computer_clipboard_write_delivery_failure(
            "Runner clipboard write ended without a recognized structured result",
            request_dispatched,
            context,
        ),
    }
}

pub(super) fn computer_effect_not_started(message: &str) -> ToolResult {
    ToolResult::err_with_output(
        message.to_string(),
        json!({
            "error_kind": "not_started",
            "message": bounded_text(message),
            "state_changed": false,
            "execution_state": "not_started"
        }),
    )
}

pub(super) fn computer_effect_outcome_unknown(message: &str) -> ToolResult {
    ToolResult::err_with_output(
        message.to_string(),
        json!({
            "error_kind": "outcome_unknown",
            "message": bounded_text(message),
            "execution_state": "outcome_unknown"
        }),
    )
    .with_recovery(RecoveryKind::Reobserve)
}

pub(super) fn computer_effect_delivery_failure(
    message: &str,
    request_dispatched: Option<bool>,
) -> ToolResult {
    if request_dispatched == Some(false) {
        computer_effect_not_started(message)
    } else {
        computer_effect_outcome_unknown(&format!(
            "{message}; the action may have taken effect, so inspect current UI state before retrying"
        ))
    }
}

pub(super) fn computer_effect_validated_result(
    result: ToolResult,
    inconsistent_message: &str,
) -> ToolResult {
    if result.success {
        result
    } else {
        computer_effect_outcome_unknown(inconsistent_message)
    }
}

pub(super) fn computer_application_effect_not_started(
    error_kind: &str,
    message: &str,
    client_id: &str,
    application_id: &str,
) -> ToolResult {
    let application_id = valid_application_id(application_id).then(|| application_id.to_string());
    let result = ToolResult::err_with_output(
        message.to_string(),
        json!({
            "error_kind": error_kind,
            "message": bounded_text(message),
            "application_id": application_id,
            "state_changed": false,
            "execution_state": "not_started",
        }),
    );
    match error_kind {
        "stale_application" => computer_observe_suggested_recovery(
            result,
            "applications",
            json!({"client_id": client_id}),
        ),
        "invalid_request" => result.with_recovery(RecoveryKind::FixInput),
        "permission_denied" => result.with_recovery(RecoveryKind::UserAction),
        _ => result,
    }
}

pub(super) fn computer_application_effect_outcome_unknown(
    message: &str,
    client_id: &str,
    application_id: &str,
) -> ToolResult {
    let safe_message = format!(
        "{message}; do not blindly retry. Reconcile with computer_observe(action=windows) first"
    );
    let result = ToolResult::err_with_output(
        safe_message.clone(),
        json!({
            "error_kind": "outcome_unknown",
            "message": bounded_text(&safe_message),
            "application_id": application_id,
            "execution_state": "outcome_unknown",
        }),
    );
    computer_observe_suggested_recovery(result, "windows", json!({"client_id": client_id}))
}

pub(super) fn computer_application_effect_delivery_failure(
    message: &str,
    request_dispatched: Option<bool>,
    client_id: &str,
    application_id: &str,
) -> ToolResult {
    if request_dispatched == Some(false) {
        computer_application_effect_not_started("not_started", message, client_id, application_id)
    } else {
        computer_application_effect_outcome_unknown(message, client_id, application_id)
    }
}

pub(super) fn computer_application_launch_runner_error(
    error: &str,
    request_dispatched: Option<bool>,
    client_id: &str,
    application_id: &str,
) -> ToolResult {
    match classify_runner_error(error) {
        "stale_application" => computer_application_effect_not_started(
            "stale_application",
            "application_id is stale; run computer_observe(action=applications) again before another launch",
            client_id,
            application_id,
        ),
        "invalid_request" => computer_application_effect_not_started(
            "invalid_request",
            "Runner rejected the application launch request before native dispatch",
            client_id,
            application_id,
        ),
        "unsupported_platform" => computer_application_effect_not_started(
            "unsupported_platform",
            "application launch is unsupported by the target platform",
            client_id,
            application_id,
        ),
        "application_failed" => computer_application_effect_not_started(
            "application_failed",
            "Native application identity could not be revalidated before native dispatch",
            client_id,
            application_id,
        ),
        "outcome_unknown" => computer_application_effect_outcome_unknown(
            "Runner reported an uncertain native application launch outcome",
            client_id,
            application_id,
        ),
        _ => computer_application_effect_delivery_failure(
            "Runner application launch ended without a recognized structured result",
            request_dispatched,
            client_id,
            application_id,
        ),
    }
}

pub(super) fn computer_text_input_runner_error(
    error: &str,
    request_dispatched: Option<bool>,
    client_id: &str,
) -> ToolResult {
    let error_kind = classify_runner_error(error);
    match error_kind {
        "outcome_unknown" => computer_effect_outcome_unknown(
            "Runner reported an uncertain computer text input outcome; inspect current UI state before retrying",
        ),
        "runner_error" => computer_effect_delivery_failure(
            "Runner computer text input ended without a recognized structured error",
            request_dispatched,
        ),
        "permission_denied" => computer_error(
            error_kind,
            "Runner denied the bounded computer text input request",
        ),
        "stale_surface" => computer_error_with_client(
            error_kind,
            "Computer text input surface is stale",
            Some(client_id),
        ),
        "stale_element" => computer_error(error_kind, "Computer text input element is stale"),
        "unsupported_platform" => computer_error(
            error_kind,
            "Computer text input is unsupported by the target platform",
        ),
        "invalid_request" => computer_error(error_kind, "Runner rejected the computer text input request"),
        "accessibility_failed" | "input_failed" => computer_error(
            error_kind,
            "Runner rejected computer text input before a successful native text write",
        ),
        _ => computer_error(error_kind, "Runner rejected computer text input"),
    }
}

pub(super) fn classify_runner_error(error: &str) -> &'static str {
    for kind in [
        "permission_denied",
        "stale_surface",
        "stale_element",
        "stale_application",
        "stale_display",
        "stale_snapshot_generation",
        "unsupported_platform",
        "application_failed",
        "display_failed",
        "capture_failed",
        "accessibility_failed",
        "control_failed",
        "scroll_failed",
        "key_input_failed",
        "pointer_input_failed",
        "clipboard_busy",
        "clipboard_too_large",
        "clipboard_malformed",
        "clipboard_failed",
        "not_started",
        "input_failed",
        "outcome_unknown",
        "image_too_large",
        "invalid_request",
    ] {
        if error.starts_with(kind) {
            return kind;
        }
    }
    "runner_error"
}

pub(super) fn bounded_text(value: &str) -> String {
    let mut end = value.len().min(MAX_TEXT_BYTES);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

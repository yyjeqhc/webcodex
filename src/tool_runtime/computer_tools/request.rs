//! Exact Runner access, capability checks, dispatch and bounded response waiting.

use super::accessibility::normalize_accessibility_scope;
use super::effects::{
    classify_runner_error, computer_application_effect_delivery_failure,
    computer_application_effect_not_started, computer_application_launch_runner_error,
    computer_clipboard_write_delivery_failure, computer_clipboard_write_not_started,
    computer_clipboard_write_runner_error, computer_effect_delivery_failure,
    computer_effect_not_started, computer_effect_outcome_unknown, computer_error,
    computer_error_recovery_message, computer_error_with_client,
    computer_pointer_effect_delivery_failure, computer_pointer_effect_not_started,
    computer_pointer_runner_error, computer_request_is_effect, computer_text_input_runner_error,
    ClipboardWriteContext, PointerRequestContext,
};
use super::responses::{validate_computer_response, ComputerResponseContext};
use super::*;

fn parse_computer_runner_output(
    stdout: Option<&str>,
    stdout_truncated: bool,
) -> Result<Value, String> {
    let Some(stdout) = stdout else {
        return Err(format!(
            "Runner returned invalid computer JSON: stdout_present=false; stdout_truncated={stdout_truncated}"
        ));
    };
    serde_json::from_str(stdout).map_err(|error| {
        // Parser Display may quote source content. Only its closed category and
        // numeric location belong in this exceptional diagnostic.
        format!(
            "Runner returned invalid computer JSON: stdout_present=true; stdout_bytes={}; stdout_truncated={stdout_truncated}; parse_category={:?}; line={}; column={}",
            stdout.len(), error.classify(), error.line(), error.column()
        )
    })
}

impl ToolRuntime {
    pub(super) async fn dispatch_computer_request(
        &self,
        client_id: &str,
        kind: &'static str,
        payload: Value,
        auth: Option<&AuthContext>,
        list_limit: Option<usize>,
        expected_surface_id: Option<&str>,
        accessibility_bounds: Option<(usize, usize)>,
    ) -> ToolResult {
        self.dispatch_computer_request_for_instance(
            client_id,
            kind,
            payload,
            auth,
            list_limit,
            expected_surface_id,
            accessibility_bounds,
            None,
        )
        .await
    }

    pub(super) async fn dispatch_computer_request_for_instance(
        &self,
        client_id: &str,
        kind: &'static str,
        payload: Value,
        auth: Option<&AuthContext>,
        list_limit: Option<usize>,
        expected_surface_id: Option<&str>,
        accessibility_bounds: Option<(usize, usize)>,
        expected_instance: Option<&str>,
    ) -> ToolResult {
        let is_letter_shortcut = kind == "computer_key_input"
            && payload
                .get("key")
                .and_then(Value::as_str)
                .is_some_and(webcodex_core::computer_input_contract::is_computer_letter_shortcut);
        let expected_root_element_id = payload
            .get("root_element_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let expected_application_id = payload
            .get("application_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let is_application_launch = kind == "computer_launch_application";
        let expected_display_id = payload
            .get("display_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let is_pointer = matches!(kind, "computer_pointer_move" | "computer_pointer_click");
        let pointer_context = is_pointer.then(|| PointerRequestContext {
            client_id: client_id.to_string(),
            display_id: expected_display_id.clone().unwrap_or_default(),
            snapshot_generation: payload
                .get("snapshot_generation")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or_default(),
            x: payload
                .get("x")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or_default(),
            y: payload
                .get("y")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or_default(),
        });
        let is_clipboard_write = kind == "computer_write_clipboard";
        let clipboard_write_context = is_clipboard_write.then(|| ClipboardWriteContext {
            text_bytes: payload.get("text").and_then(Value::as_str).map(str::len),
        });
        if client_id.is_empty() || client_id.len() > 128 {
            if is_application_launch {
                return computer_application_effect_not_started(
                    "invalid_client",
                    "client_id is invalid",
                    client_id,
                    expected_application_id.as_deref().unwrap_or_default(),
                );
            }
            if let Some(context) = pointer_context.as_ref() {
                return computer_pointer_effect_not_started(
                    "invalid_client",
                    "client_id is invalid",
                    context,
                );
            }
            if let Some(context) = clipboard_write_context.as_ref() {
                return computer_clipboard_write_not_started(
                    "invalid_client",
                    "client_id is invalid",
                    context,
                );
            }
            return computer_error("invalid_client", "client_id is invalid");
        }
        let required_features: &[RunnerFeature] = match kind {
            "computer_list_applications" => &[RunnerFeature::ComputerApplicationDiscovery],
            "computer_launch_application" => &[RunnerFeature::ComputerApplicationLaunch],
            "computer_list_displays" | "computer_snapshot_display" => {
                &[RunnerFeature::ComputerDisplayObserve]
            }
            "computer_read_clipboard" => &[RunnerFeature::ComputerClipboardRead],
            "computer_write_clipboard" => &[RunnerFeature::ComputerClipboardWrite],
            "computer_pointer_move" | "computer_pointer_click" => {
                &[RunnerFeature::ComputerPointerControl]
            }
            "computer_list_windows" | "computer_snapshot" => &[RunnerFeature::ComputerObserve],
            "computer_snapshot_region" => &[
                RunnerFeature::ComputerObserve,
                RunnerFeature::ComputerSnapshotRegion,
            ],
            "computer_accessibility_status" | "computer_accessibility_tree" => {
                &[RunnerFeature::ComputerAccessibilityObserve]
            }
            "computer_element_state" => &[RunnerFeature::ComputerElementState],
            "computer_control" => &[RunnerFeature::ComputerControl],
            "computer_scroll_to_element" => &[RunnerFeature::ComputerScrollToElement],
            "computer_key_input" => {
                webcodex_core::computer_input_contract::computer_key_input_capabilities(
                    payload
                        .get("key")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                )
            }
            "computer_activate_window" => &[RunnerFeature::ComputerWindowActivate],
            "computer_input_text" => &[RunnerFeature::ComputerTextInput],
            _ => return computer_error("invalid_request", "unsupported computer request kind"),
        };
        let client = match self
            .runner_registry
            .get_runner_semantic_view_checked_for_auth(
                client_id,
                crate::runner_http::runner_access_from_auth(auth).as_ref(),
            )
            .await
        {
            Ok(client) => client,
            Err(_error) if is_application_launch => {
                return computer_application_effect_not_started(
                    "client_access_denied",
                    "caller cannot access the target Runner for application launch",
                    client_id,
                    expected_application_id.as_deref().unwrap_or_default(),
                );
            }
            Err(_error) if is_pointer => {
                return computer_pointer_effect_not_started(
                    "client_access_denied",
                    "caller cannot access the target Runner for pointer control",
                    pointer_context.as_ref().expect("pointer context"),
                );
            }
            Err(_error) if is_clipboard_write => {
                return computer_clipboard_write_not_started(
                    "client_access_denied",
                    "caller cannot access the target Runner for clipboard write",
                    clipboard_write_context
                        .as_ref()
                        .expect("clipboard write context"),
                );
            }
            Err(error)
                if (expected_instance.is_some() || is_letter_shortcut)
                    && computer_request_is_effect(kind) =>
            {
                return computer_effect_not_started(&error);
            }
            Err(error) => return computer_error("client_access_denied", &error),
        };
        // Pin new letter chords to the registration whose capability was observed.
        // The same fence is checked under the queue lock and again at dequeue.
        let expected_instance = expected_instance
            .or_else(|| is_letter_shortcut.then_some(client.view.runner_instance_id.as_str()));
        for required_feature in required_features {
            if client.supports(*required_feature) {
                continue;
            }
            let required_capability = required_feature.as_wire_name();
            if expected_instance.is_some() && computer_request_is_effect(kind) {
                return computer_effect_not_started(&format!(
                    "target Runner does not support {required_capability}"
                ));
            }
            if is_application_launch {
                return computer_application_effect_not_started(
                    "capability_unavailable",
                    &format!("target Runner does not support {required_capability}"),
                    client_id,
                    expected_application_id.as_deref().unwrap_or_default(),
                );
            }
            if let Some(context) = pointer_context.as_ref() {
                return computer_pointer_effect_not_started(
                    "capability_unavailable",
                    &format!("target Runner does not support {required_capability}"),
                    context,
                );
            }
            if let Some(context) = clipboard_write_context.as_ref() {
                return computer_clipboard_write_not_started(
                    "capability_unavailable",
                    &format!("target Runner does not support {required_capability}"),
                    context,
                );
            }
            return computer_error(
                "capability_unavailable",
                &format!("target Runner does not support {required_capability}"),
            );
        }
        let expected_element_id = payload
            .get("element_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let expected_action = payload
            .get("action")
            .and_then(Value::as_str)
            .map(str::to_string);
        let expected_key = payload
            .get("key")
            .and_then(Value::as_str)
            .map(str::to_string);
        let expected_key_modifiers = payload
            .get("modifiers")
            .cloned()
            .unwrap_or_else(|| json!([]));
        let expected_text_bytes = payload.get("text").and_then(Value::as_str).map(str::len);
        let snapshot_advanced = kind == "computer_snapshot_region";
        let expected_snapshot_region = payload
            .get("region")
            .filter(|value| !value.is_null())
            .cloned();
        let expected_snapshot_max_width = payload.get("max_width").and_then(Value::as_u64);
        let expected_snapshot_max_height = payload.get("max_height").and_then(Value::as_u64);
        let payload = match serde_json::to_string(&payload) {
            Ok(payload) => payload,
            Err(_) if is_application_launch => {
                return computer_application_effect_not_started(
                    "invalid_request",
                    "could not encode application launch request",
                    client_id,
                    expected_application_id.as_deref().unwrap_or_default(),
                );
            }
            Err(_) if is_pointer => {
                return computer_pointer_effect_not_started(
                    "invalid_request",
                    "could not encode pointer control request",
                    pointer_context.as_ref().expect("pointer context"),
                );
            }
            Err(_) if is_clipboard_write => {
                return computer_clipboard_write_not_started(
                    "invalid_request",
                    "could not encode clipboard write request",
                    clipboard_write_context
                        .as_ref()
                        .expect("clipboard write context"),
                );
            }
            Err(_) => {
                return computer_error("invalid_request", "could not encode computer request")
            }
        };
        let requested_by = crate::runner_http::requested_by_from_auth(auth);
        let enqueued = if let Some(instance) = expected_instance {
            self.runner_registry
                .enqueue_computer_for_instance(
                    client_id.to_string(),
                    kind,
                    payload,
                    requested_by,
                    crate::runner_http::runner_access_from_auth(auth).as_ref(),
                    COMPUTER_WAIT_SECS,
                    instance,
                )
                .await
        } else {
            self.runner_registry
                .enqueue_computer(
                    client_id.to_string(),
                    kind,
                    payload,
                    requested_by,
                    crate::runner_http::runner_access_from_auth(auth).as_ref(),
                    COMPUTER_WAIT_SECS,
                )
                .await
        };
        let (request_id, mut receiver) = match enqueued {
            Ok(value) => value,
            Err(error) if kind == "computer_launch_application" => {
                return computer_application_effect_not_started(
                    "not_started",
                    &format!("application launch request was not dispatched: {error}"),
                    client_id,
                    expected_application_id.as_deref().unwrap_or_default(),
                )
            }
            Err(error) if is_pointer => {
                return computer_pointer_effect_not_started(
                    "not_started",
                    &format!("pointer request was not dispatched: {error}"),
                    pointer_context.as_ref().expect("pointer context"),
                );
            }
            Err(error) if is_clipboard_write => {
                return computer_clipboard_write_not_started(
                    "not_started",
                    &format!("clipboard write request was not dispatched: {error}"),
                    clipboard_write_context
                        .as_ref()
                        .expect("clipboard write context"),
                );
            }
            Err(error) if expected_instance.is_some() && computer_request_is_effect(kind) => {
                return computer_effect_not_started(&error);
            }
            Err(error) => return computer_error("dispatch_denied", &error),
        };
        let is_effect = computer_request_is_effect(kind);
        let is_text_input = kind == "computer_input_text";
        // Timeout cleanup still owns the response waiter until it has read
        // dispatch certainty under the Registry lock. Dropping it earlier lets
        // queue polling or an abandoned-request sweep erase that evidence.
        let response = match tokio::time::timeout(
            Duration::from_secs(COMPUTER_WAIT_SECS + 2),
            &mut receiver,
        )
        .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(_)) if is_effect => {
                let request_dispatched = self
                    .runner_registry
                    .cancel_request_dispatch_state(&request_id)
                    .await;
                if is_application_launch {
                    return computer_application_effect_delivery_failure(
                        "Runner response channel closed before a terminal application launch result was received",
                        request_dispatched,
                        client_id,
                        expected_application_id.as_deref().unwrap_or_default(),
                    );
                }
                if is_pointer {
                    return computer_pointer_effect_delivery_failure(
                        "Runner response channel closed before a terminal pointer result was received",
                        request_dispatched,
                        pointer_context.as_ref().expect("pointer context"),
                    );
                }
                if is_clipboard_write {
                    return computer_clipboard_write_delivery_failure(
                        "Runner response channel closed before a terminal clipboard write result was received",
                        request_dispatched,
                        clipboard_write_context.as_ref().expect("clipboard write context"),
                    );
                }
                return computer_effect_delivery_failure(
                    "Runner response channel closed before a terminal computer effect result was received",
                    request_dispatched,
                );
            }
            Ok(Err(_)) => {
                return computer_error("runner_disconnected", "Runner response channel closed")
            }
            Err(_) if is_effect => {
                let request_dispatched = self
                    .runner_registry
                    .cancel_request_dispatch_state(&request_id)
                    .await;
                if is_application_launch {
                    return computer_application_effect_delivery_failure(
                        "Runner did not return a terminal application launch result in time",
                        request_dispatched,
                        client_id,
                        expected_application_id.as_deref().unwrap_or_default(),
                    );
                }
                if is_pointer {
                    return computer_pointer_effect_delivery_failure(
                        "Runner did not return a terminal pointer result in time",
                        request_dispatched,
                        pointer_context.as_ref().expect("pointer context"),
                    );
                }
                if is_clipboard_write {
                    return computer_clipboard_write_delivery_failure(
                        "Runner did not return a terminal clipboard write result in time",
                        request_dispatched,
                        clipboard_write_context
                            .as_ref()
                            .expect("clipboard write context"),
                    );
                }
                return computer_effect_delivery_failure(
                    "Runner did not return a terminal computer effect result in time",
                    request_dispatched,
                );
            }
            Err(_) => {
                return computer_error(
                    "runner_timeout",
                    "Runner did not return computer request in time",
                )
            }
        };
        if let Some(error) = response.error.as_deref() {
            let error_kind = classify_runner_error(error);
            if is_text_input {
                return computer_text_input_runner_error(
                    error,
                    response.request_dispatched,
                    client_id,
                );
            }
            if is_pointer {
                return computer_pointer_runner_error(
                    error,
                    response.request_dispatched,
                    pointer_context.as_ref().expect("pointer context"),
                );
            }
            if is_clipboard_write {
                return computer_clipboard_write_runner_error(
                    error,
                    response.request_dispatched,
                    clipboard_write_context
                        .as_ref()
                        .expect("clipboard write context"),
                );
            }
            if is_application_launch {
                return computer_application_launch_runner_error(
                    error,
                    response.request_dispatched,
                    client_id,
                    expected_application_id.as_deref().unwrap_or_default(),
                );
            }
            if is_effect && error_kind == "outcome_unknown" {
                return computer_effect_outcome_unknown(error);
            }
            if is_effect && error_kind == "runner_error" {
                return computer_effect_delivery_failure(error, response.request_dispatched);
            }
            return computer_error_with_client(
                error_kind,
                &computer_error_recovery_message(error_kind, error),
                Some(client_id),
            );
        }
        if response.exit_code != Some(0) {
            if is_pointer {
                return computer_pointer_effect_delivery_failure(
                    "Runner pointer effect ended without a structured terminal result",
                    response.request_dispatched,
                    pointer_context.as_ref().expect("pointer context"),
                );
            }
            if is_application_launch {
                return computer_application_effect_delivery_failure(
                    "Runner application launch ended without a structured terminal result",
                    response.request_dispatched,
                    client_id,
                    expected_application_id.as_deref().unwrap_or_default(),
                );
            }
            if is_clipboard_write {
                return computer_clipboard_write_delivery_failure(
                    "Runner clipboard write ended without a structured terminal result",
                    response.request_dispatched,
                    clipboard_write_context
                        .as_ref()
                        .expect("clipboard write context"),
                );
            }
            if is_effect {
                return computer_effect_delivery_failure(
                    "Runner computer effect ended without a structured terminal result",
                    response.request_dispatched,
                );
            }
            return computer_error("runner_error", "Runner computer request failed");
        }
        let mut output = match parse_computer_runner_output(
            response.stdout.as_deref(),
            response.stdout_truncated,
        ) {
            Ok(output) => output,
            _ if is_pointer => {
                return computer_pointer_effect_delivery_failure(
                    "Runner returned invalid JSON after possible pointer dispatch",
                    response.request_dispatched,
                    pointer_context.as_ref().expect("pointer context"),
                )
            }
            _ if is_application_launch => {
                return computer_application_effect_delivery_failure(
                    "Runner returned invalid JSON after possible application launch dispatch",
                    response.request_dispatched,
                    client_id,
                    expected_application_id.as_deref().unwrap_or_default(),
                )
            }
            _ if is_clipboard_write => {
                return computer_clipboard_write_delivery_failure(
                    "Runner returned invalid JSON after possible clipboard replacement",
                    response.request_dispatched,
                    clipboard_write_context
                        .as_ref()
                        .expect("clipboard write context"),
                )
            }
            _ if is_effect => {
                return computer_effect_delivery_failure(
                    "Runner returned invalid JSON after computer effect execution",
                    response.request_dispatched,
                )
            }
            Err(message) => return computer_error("invalid_runner_response", &message),
        };
        if kind == "computer_accessibility_tree" {
            if let Err(message) =
                normalize_accessibility_scope(&mut output, expected_root_element_id.as_deref())
            {
                return computer_error("invalid_runner_response", &message);
            }
        }
        validate_computer_response(
            output,
            ComputerResponseContext {
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
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::parse_computer_runner_output;
    use serde_json::Value;

    #[test]
    fn computer_json_diagnostics_are_bounded_and_never_quote_payloads() {
        for truncated in [false, true] {
            let missing = parse_computer_runner_output(None, truncated).unwrap_err();
            assert!(missing.contains("stdout_present=false"));
            assert!(missing.contains(&format!("stdout_truncated={truncated}")));
            for (raw, category) in [
                ("", "Eof"),
                ("{\n\"PRIVATE_SCREENSHOT\":", "Eof"),
                ("{\"秘密PRIVATE_SCREENSHOT\":?}", "Syntax"),
            ] {
                let error = serde_json::from_str::<Value>(raw).unwrap_err();
                let diagnostic = parse_computer_runner_output(Some(raw), truncated).unwrap_err();
                assert!(diagnostic.contains(&format!("stdout_bytes={}", raw.len())));
                assert!(diagnostic.contains(&format!("stdout_truncated={truncated}")));
                assert!(diagnostic.contains(&format!("parse_category={category}")));
                assert!(diagnostic.contains(&format!(
                    "line={}; column={}",
                    error.line(),
                    error.column()
                )));
                assert!(!diagnostic.contains("PRIVATE_SCREENSHOT"));
                assert!(!diagnostic.contains("秘密"));
                assert!(diagnostic.len() < 256);
            }
        }
    }

    #[test]
    fn computer_json_null_reaches_semantic_validation() {
        assert_eq!(
            parse_computer_runner_output(Some("null"), false).unwrap(),
            Value::Null
        );
        assert_eq!(
            parse_computer_runner_output(Some("{}"), false).unwrap(),
            serde_json::json!({})
        );
    }
}

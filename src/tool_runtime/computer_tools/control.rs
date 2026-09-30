//! Consequential Computer action admission and routing.

use super::effects::{
    computer_application_effect_not_started, computer_effect_not_started, computer_error,
    computer_pointer_effect_not_started, PointerRequestContext,
};
use super::inputs::{
    normalize_computer_key_input, valid_application_id, valid_display_id,
    validate_clipboard_write_text, validate_input_text,
};
use super::*;
impl ToolRuntime {
    pub(super) async fn dispatch_computer_control(
        &self,
        call: ComputerControlToolCall,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        match call {
            ComputerControlToolCall::LaunchApplication {
                client_id,
                application_id,
            } => {
                if !valid_application_id(&application_id) {
                    return computer_application_effect_not_started(
                        "invalid_application",
                        "application_id is invalid",
                        &client_id,
                        &application_id,
                    );
                }
                self.dispatch_computer_request(
                    &client_id,
                    "computer_launch_application",
                    json!({"application_id": application_id}),
                    auth,
                    None,
                    None,
                    None,
                )
                .await
            }
            ComputerControlToolCall::ActivateWindow {
                client_id,
                surface_id,
            } => {
                if surface_id.is_empty() || surface_id.len() > MAX_SURFACE_ID_BYTES {
                    return computer_error("invalid_surface", "surface_id is invalid");
                }
                self.dispatch_computer_request(
                    &client_id,
                    "computer_activate_window",
                    json!({"surface_id": surface_id}),
                    auth,
                    None,
                    Some(surface_id.as_str()),
                    None,
                )
                .await
            }

            call @ (ComputerControlToolCall::Press { .. }
            | ComputerControlToolCall::Focus { .. }) => {
                let (client_id, surface_id, element_id, action) = match call {
                    ComputerControlToolCall::Press {
                        client_id,
                        surface_id,
                        element_id,
                    } => (client_id, surface_id, element_id, "press"),
                    ComputerControlToolCall::Focus {
                        client_id,
                        surface_id,
                        element_id,
                    } => (client_id, surface_id, element_id, "focus"),
                    _ => unreachable!("matched Computer element-control action"),
                };
                if surface_id.is_empty() || surface_id.len() > MAX_SURFACE_ID_BYTES {
                    return computer_error("invalid_surface", "surface_id is invalid");
                }
                if !element_id.starts_with("element_")
                    || element_id.len() <= "element_".len()
                    || element_id.len() > MAX_ELEMENT_ID_BYTES
                {
                    return computer_error("invalid_element", "element_id is invalid");
                }
                self.dispatch_computer_request(
                    &client_id,
                    "computer_control",
                    json!({
                        "surface_id": surface_id,
                        "element_id": element_id,
                        "action": action,
                    }),
                    auth,
                    None,
                    Some(surface_id.as_str()),
                    None,
                )
                .await
            }
            ComputerControlToolCall::ScrollToElement {
                client_id,
                surface_id,
                element_id,
            } => {
                if surface_id.is_empty() || surface_id.len() > MAX_SURFACE_ID_BYTES {
                    return computer_error("invalid_surface", "surface_id is invalid");
                }
                if !element_id.starts_with("element_")
                    || element_id.len() <= "element_".len()
                    || element_id.len() > MAX_ELEMENT_ID_BYTES
                {
                    return computer_error("invalid_element", "element_id is invalid");
                }
                self.dispatch_computer_request(
                    &client_id,
                    "computer_scroll_to_element",
                    json!({"surface_id": surface_id, "element_id": element_id}),
                    auth,
                    None,
                    Some(surface_id.as_str()),
                    None,
                )
                .await
            }
            ComputerControlToolCall::Key {
                client_id,
                surface_id,
                key,
                modifiers,
            } => {
                if surface_id.is_empty() || surface_id.len() > MAX_SURFACE_ID_BYTES {
                    return computer_error("invalid_surface", "surface_id is invalid");
                }
                let modifiers = match normalize_computer_key_input(&key, modifiers) {
                    Ok(modifiers) => modifiers,
                    Err(message) => return computer_error("invalid_request", message),
                };
                self.dispatch_computer_request(
                    &client_id,
                    "computer_key_input",
                    json!({"surface_id": surface_id, "key": key, "modifiers": modifiers}),
                    auth,
                    None,
                    Some(surface_id.as_str()),
                    None,
                )
                .await
            }
            ComputerControlToolCall::WriteClipboard { client_id, text } => {
                if let Err(message) = validate_clipboard_write_text(&text) {
                    return computer_effect_not_started(message);
                }
                self.dispatch_computer_request(
                    &client_id,
                    "computer_write_clipboard",
                    json!({"text": text}),
                    auth,
                    None,
                    None,
                    None,
                )
                .await
            }
            ComputerControlToolCall::PointerMove {
                client_id,
                display_id,
                snapshot_generation,
                x,
                y,
            } => {
                let context = PointerRequestContext {
                    client_id: client_id.clone(),
                    display_id: display_id.clone(),
                    snapshot_generation,
                    x,
                    y,
                };
                if !valid_display_id(&display_id) {
                    return computer_pointer_effect_not_started(
                        "invalid_display",
                        "display_id is invalid",
                        &context,
                    );
                }
                if snapshot_generation == 0 {
                    return computer_pointer_effect_not_started(
                        "invalid_request",
                        "snapshot_generation must be positive",
                        &context,
                    );
                }
                self.dispatch_computer_request(
                    &client_id,
                    "computer_pointer_move",
                    json!({"display_id": display_id, "snapshot_generation": snapshot_generation, "x": x, "y": y}),
                    auth,
                    None,
                    None,
                    None,
                )
                .await
            }
            ComputerControlToolCall::PointerClick {
                client_id,
                display_id,
                snapshot_generation,
                x,
                y,
            } => {
                let context = PointerRequestContext {
                    client_id: client_id.clone(),
                    display_id: display_id.clone(),
                    snapshot_generation,
                    x,
                    y,
                };
                if !valid_display_id(&display_id) {
                    return computer_pointer_effect_not_started(
                        "invalid_display",
                        "display_id is invalid",
                        &context,
                    );
                }
                if snapshot_generation == 0 {
                    return computer_pointer_effect_not_started(
                        "invalid_request",
                        "snapshot_generation must be positive",
                        &context,
                    );
                }
                self.dispatch_computer_request(
                    &client_id,
                    "computer_pointer_click",
                    json!({"display_id": display_id, "snapshot_generation": snapshot_generation, "x": x, "y": y}),
                    auth,
                    None,
                    None,
                    None,
                )
                .await
            }
            ComputerControlToolCall::InputText {
                client_id,
                surface_id,
                element_id,
                text,
            } => {
                if surface_id.is_empty() || surface_id.len() > MAX_SURFACE_ID_BYTES {
                    return computer_error("invalid_surface", "surface_id is invalid");
                }
                if !element_id.starts_with("element_")
                    || element_id.len() <= "element_".len()
                    || element_id.len() > MAX_ELEMENT_ID_BYTES
                {
                    return computer_error("invalid_element", "element_id is invalid");
                }
                if let Err(message) = validate_input_text(&text) {
                    return computer_error("invalid_request", message);
                }
                self.dispatch_computer_request(
                    &client_id,
                    "computer_input_text",
                    json!({
                        "surface_id": surface_id,
                        "element_id": element_id,
                        "text": text,
                    }),
                    auth,
                    None,
                    Some(surface_id.as_str()),
                    None,
                )
                .await
            }
        }
    }
}

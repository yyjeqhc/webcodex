use super::files::{validate_artifact_file_path, validate_artifact_mime_for_path};
use super::shell::{agent_command_lifecycle, dispatch_uncertainty_lifecycle};
use super::tool_call::ComputerSnapshotRegion;
use super::{ToolCall, ToolResult, ToolRuntime};
use crate::artifact_policy::MAX_MCP_IMAGE_BYTES;
use crate::auth::AuthContext;
use crate::shell_protocol::{
    ShellCommandExecutionState, ShellFileOpRequest,
    SHELL_CLIENT_CAPABILITY_COMPUTER_APPLICATION_DISCOVERY,
    SHELL_CLIENT_CAPABILITY_COMPUTER_APPLICATION_LAUNCH,
    SHELL_CLIENT_CAPABILITY_COMPUTER_CLIPBOARD_READ,
    SHELL_CLIENT_CAPABILITY_COMPUTER_CLIPBOARD_WRITE, SHELL_CLIENT_CAPABILITY_COMPUTER_CONTROL,
    SHELL_CLIENT_CAPABILITY_COMPUTER_DISPLAY_OBSERVE,
    SHELL_CLIENT_CAPABILITY_COMPUTER_ELEMENT_STATE, SHELL_CLIENT_CAPABILITY_COMPUTER_KEY_INPUT,
    SHELL_CLIENT_CAPABILITY_COMPUTER_OBSERVE, SHELL_CLIENT_CAPABILITY_COMPUTER_POINTER_CONTROL,
    SHELL_CLIENT_CAPABILITY_COMPUTER_SCROLL_TO_ELEMENT,
    SHELL_CLIENT_CAPABILITY_COMPUTER_SNAPSHOT_REGION, SHELL_CLIENT_CAPABILITY_COMPUTER_TEXT_INPUT,
    SHELL_CLIENT_CAPABILITY_COMPUTER_WINDOW_ACTIVATE,
};
use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::Duration;

const MAX_WINDOWS: usize = 64;
const MAX_APPLICATIONS: usize = 64;
const MAX_DISPLAYS: usize = 16;
const MAX_APPLICATION_ID_BYTES: usize = 128;
const MAX_DISPLAY_ID_BYTES: usize = 128;
const MAX_RAW_CAPTURE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 256;
const MAX_SURFACE_ID_BYTES: usize = 128;
const MAX_ELEMENT_ID_BYTES: usize = 128;
const MAX_INPUT_TEXT_BYTES: usize = 2048;
const MAX_CLIPBOARD_TEXT_BYTES: usize = 16 * 1024;
const MAX_ACCESSIBILITY_DEPTH: usize = 8;
const MAX_ACCESSIBILITY_NODES: usize = 256;
const DEFAULT_ACCESSIBILITY_DEPTH: usize = 6;
const DEFAULT_ACCESSIBILITY_NODES: usize = 128;
const MAX_ACCESSIBILITY_CHILD_COUNT: u64 = 1_000_000;
const MAX_IMAGE_DIMENSION: u64 = 4096;
const COMPUTER_WAIT_SECS: u64 = 30;
const MAX_COMPUTER_TARGETS: usize = 64;
const DEFAULT_FIND_ELEMENTS_LIMIT: usize = 8;
const MAX_FIND_ELEMENTS_LIMIT: usize = 32;
const COMPUTER_KEY_INPUT_KEYS: &[&str] = &[
    "enter",
    "escape",
    "tab",
    "arrow_up",
    "arrow_down",
    "arrow_left",
    "arrow_right",
    "page_up",
    "page_down",
    "home",
    "end",
];
const COMPUTER_KEY_INPUT_MODIFIERS: &[&str] = &["shift", "control", "option", "command"];

fn normalize_computer_key_input(
    key: &str,
    modifiers: Option<Vec<String>>,
) -> Result<Vec<String>, &'static str> {
    if !COMPUTER_KEY_INPUT_KEYS.contains(&key) {
        return Err("computer key input key is outside the closed vocabulary");
    }
    let mut modifiers = modifiers.unwrap_or_default();
    if modifiers.len() > COMPUTER_KEY_INPUT_MODIFIERS.len() {
        return Err("computer key input has too many modifiers");
    }
    for (index, modifier) in modifiers.iter().enumerate() {
        if !COMPUTER_KEY_INPUT_MODIFIERS.contains(&modifier.as_str())
            || modifiers[..index].contains(modifier)
        {
            return Err("computer key input modifiers are invalid or duplicated");
        }
    }
    modifiers.sort_by_key(|modifier| {
        COMPUTER_KEY_INPUT_MODIFIERS
            .iter()
            .position(|allowed| *allowed == modifier)
            .unwrap_or(COMPUTER_KEY_INPUT_MODIFIERS.len())
    });
    Ok(modifiers)
}

fn validate_clipboard_write_text(text: &str) -> Result<usize, &'static str> {
    if text.is_empty() {
        return Err("clipboard text must not be empty");
    }
    if text.contains('\0') {
        return Err("clipboard text must not contain NUL");
    }
    if text.len() > MAX_CLIPBOARD_TEXT_BYTES {
        return Err("clipboard text exceeds the 16 KiB UTF-8 bound");
    }
    Ok(text.len())
}

fn valid_application_id(application_id: &str) -> bool {
    let Some(suffix) = application_id.strip_prefix("application_") else {
        return false;
    };
    application_id.len() <= MAX_APPLICATION_ID_BYTES
        && suffix.len() == 32
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_display_id(display_id: &str) -> bool {
    let Some(suffix) = display_id.strip_prefix("display_") else {
        return false;
    };
    display_id.len() <= MAX_DISPLAY_ID_BYTES
        && suffix.len() == 32
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_input_text(text: &str) -> Result<usize, &'static str> {
    let text_bytes = text.len();
    if text_bytes == 0 || text_bytes > MAX_INPUT_TEXT_BYTES || text.contains('\0') {
        return Err(
            "computer text input must be non-empty, NUL-free, and within the UTF-8 byte limit",
        );
    }
    Ok(text_bytes)
}

impl ToolRuntime {
    pub(super) async fn dispatch_computer_tool(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        match call {
            ToolCall::ComputerListTargets => self.computer_list_targets(auth).await,
            ToolCall::ComputerListWindows { client_id, limit } => {
                let limit = limit.unwrap_or(MAX_WINDOWS).clamp(1, MAX_WINDOWS);
                self.dispatch_computer_request(
                    &client_id,
                    "computer_list_windows",
                    json!({"limit": limit}),
                    auth,
                    Some(limit),
                    None,
                    None,
                )
                .await
            }
            ToolCall::ComputerListDisplays { client_id, limit } => {
                let limit = limit.unwrap_or(MAX_DISPLAYS).clamp(1, MAX_DISPLAYS);
                self.dispatch_computer_request(
                    &client_id,
                    "computer_list_displays",
                    json!({"limit": limit}),
                    auth,
                    Some(limit),
                    None,
                    None,
                )
                .await
            }
            ToolCall::ComputerListApplications { client_id, limit } => {
                let limit = limit.unwrap_or(MAX_APPLICATIONS).clamp(1, MAX_APPLICATIONS);
                self.dispatch_computer_request(
                    &client_id,
                    "computer_list_applications",
                    json!({"limit": limit}),
                    auth,
                    Some(limit),
                    None,
                    None,
                )
                .await
            }
            ToolCall::ComputerLaunchApplication {
                client_id,
                application_id,
            } => {
                if !valid_application_id(&application_id) {
                    return computer_application_effect_not_started(
                        "invalid_application",
                        "application_id is invalid",
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
            ToolCall::ComputerAccessibilityStatus { client_id } => {
                self.dispatch_computer_request(
                    &client_id,
                    "computer_accessibility_status",
                    json!({}),
                    auth,
                    None,
                    None,
                    None,
                )
                .await
            }
            ToolCall::ComputerAccessibilityTree {
                client_id,
                surface_id,
                max_depth,
                max_nodes,
            } => {
                if surface_id.is_empty() || surface_id.len() > MAX_SURFACE_ID_BYTES {
                    return computer_error("invalid_surface", "surface_id is invalid");
                }
                let max_depth = max_depth
                    .unwrap_or(DEFAULT_ACCESSIBILITY_DEPTH)
                    .min(MAX_ACCESSIBILITY_DEPTH);
                let max_nodes = max_nodes
                    .unwrap_or(DEFAULT_ACCESSIBILITY_NODES)
                    .clamp(1, MAX_ACCESSIBILITY_NODES);
                self.dispatch_computer_request(
                    &client_id,
                    "computer_accessibility_tree",
                    json!({
                        "surface_id": surface_id,
                        "max_depth": max_depth,
                        "max_nodes": max_nodes,
                    }),
                    auth,
                    None,
                    Some(surface_id.as_str()),
                    Some((max_depth, max_nodes)),
                )
                .await
            }
            ToolCall::ComputerFindElements {
                client_id,
                surface_id,
                role,
                subrole,
                label,
                focused,
                enabled,
                limit,
            } => {
                if surface_id.is_empty() || surface_id.len() > MAX_SURFACE_ID_BYTES {
                    return computer_error("invalid_surface", "surface_id is invalid");
                }
                for (name, value) in [
                    ("role", role.as_deref()),
                    ("subrole", subrole.as_deref()),
                    ("label", label.as_deref()),
                ] {
                    if let Some(value) = value {
                        if value.is_empty() || value.len() > MAX_TEXT_BYTES || value.contains('\0')
                        {
                            return computer_error(
                                "invalid_request",
                                &format!("computer element finder {name} filter is invalid"),
                            );
                        }
                    }
                }
                if role.is_none()
                    && subrole.is_none()
                    && label.is_none()
                    && focused.is_none()
                    && enabled.is_none()
                {
                    return computer_error(
                        "invalid_request",
                        "computer element finder requires at least one semantic or state filter",
                    );
                }
                let limit = limit
                    .unwrap_or(DEFAULT_FIND_ELEMENTS_LIMIT)
                    .clamp(1, MAX_FIND_ELEMENTS_LIMIT);
                let tree = self
                    .dispatch_computer_request(
                        &client_id,
                        "computer_accessibility_tree",
                        json!({
                            "surface_id": surface_id,
                            "max_depth": MAX_ACCESSIBILITY_DEPTH,
                            "max_nodes": MAX_ACCESSIBILITY_NODES,
                        }),
                        auth,
                        None,
                        Some(surface_id.as_str()),
                        Some((MAX_ACCESSIBILITY_DEPTH, MAX_ACCESSIBILITY_NODES)),
                    )
                    .await;
                if !tree.success {
                    return tree;
                }
                filter_accessibility_tree(
                    tree.output,
                    &surface_id,
                    role.as_deref(),
                    subrole.as_deref(),
                    label.as_deref(),
                    focused,
                    enabled,
                    limit,
                )
            }
            ToolCall::ComputerElementState {
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
                    "computer_element_state",
                    json!({"surface_id": surface_id, "element_id": element_id}),
                    auth,
                    None,
                    Some(surface_id.as_str()),
                    None,
                )
                .await
            }
            ToolCall::ComputerActivateWindow {
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
            ToolCall::ComputerControl {
                client_id,
                surface_id,
                element_id,
                action,
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
                if !matches!(action.as_str(), "press" | "focus") {
                    return computer_error("invalid_request", "computer control action is invalid");
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
            ToolCall::ComputerScrollToElement {
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
            ToolCall::ComputerKeyInput {
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
            ToolCall::ComputerReadClipboard { client_id } => {
                self.dispatch_computer_request(
                    &client_id,
                    "computer_read_clipboard",
                    json!({}),
                    auth,
                    None,
                    None,
                    None,
                )
                .await
            }
            ToolCall::ComputerWriteClipboard { client_id, text } => {
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
            ToolCall::ComputerPointerMove {
                client_id,
                display_id,
                snapshot_generation,
                x,
                y,
            } => {
                let context = PointerRequestContext {
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
            ToolCall::ComputerPointerClick {
                client_id,
                display_id,
                snapshot_generation,
                x,
                y,
            } => {
                let context = PointerRequestContext {
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
            ToolCall::ComputerInputText {
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
            ToolCall::ComputerSnapshot {
                client_id,
                surface_id,
                region,
                max_width,
                max_height,
            } => {
                self.capture_computer_snapshot(
                    &client_id,
                    &surface_id,
                    region,
                    max_width,
                    max_height,
                    auth,
                )
                .await
            }
            ToolCall::ComputerSnapshotDisplay {
                client_id,
                display_id,
                max_width,
                max_height,
            } => {
                if !valid_display_id(&display_id) {
                    return computer_error("invalid_display", "display_id is invalid");
                }
                if max_width.is_some_and(|value| value == 0 || value > MAX_IMAGE_DIMENSION as u32)
                    || max_height
                        .is_some_and(|value| value == 0 || value > MAX_IMAGE_DIMENSION as u32)
                {
                    return computer_error(
                        "invalid_request",
                        "display snapshot output dimension bound is invalid",
                    );
                }
                self.dispatch_computer_request(
                    &client_id,
                    "computer_snapshot_display",
                    json!({
                        "display_id": display_id,
                        "max_width": max_width,
                        "max_height": max_height,
                    }),
                    auth,
                    None,
                    None,
                    None,
                )
                .await
            }
            ToolCall::ComputerSaveSnapshot {
                project,
                path,
                client_id,
                surface_id,
                region,
                max_width,
                max_height,
                ..
            } => {
                self.save_computer_snapshot_artifact(
                    project, path, client_id, surface_id, region, max_width, max_height, auth,
                )
                .await
            }
            _ => ToolResult::err("invalid computer tool dispatch".to_string()),
        }
    }

    async fn capture_computer_snapshot(
        &self,
        client_id: &str,
        surface_id: &str,
        region: Option<ComputerSnapshotRegion>,
        max_width: Option<u32>,
        max_height: Option<u32>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if surface_id.is_empty() || surface_id.len() > MAX_SURFACE_ID_BYTES {
            return computer_error("invalid_surface", "surface_id is invalid");
        }
        if let Some(region) = region.as_ref() {
            if region.width == 0
                || region.height == 0
                || region.x.checked_add(region.width).is_none()
                || region.y.checked_add(region.height).is_none()
            {
                return computer_error("invalid_request", "snapshot region is invalid");
            }
        }
        if max_width.is_some_and(|value| value == 0 || value > MAX_IMAGE_DIMENSION as u32)
            || max_height.is_some_and(|value| value == 0 || value > MAX_IMAGE_DIMENSION as u32)
        {
            return computer_error(
                "invalid_request",
                "snapshot output dimension bound is invalid",
            );
        }
        let advanced = region.is_some() || max_width.is_some() || max_height.is_some();
        let (kind, payload) = if advanced {
            (
                "computer_snapshot_region",
                json!({
                    "surface_id": surface_id,
                    "region": region,
                    "max_width": max_width,
                    "max_height": max_height,
                }),
            )
        } else {
            ("computer_snapshot", json!({"surface_id": surface_id}))
        };
        self.dispatch_computer_request(client_id, kind, payload, auth, None, Some(surface_id), None)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn save_computer_snapshot_artifact(
        &self,
        project: String,
        path: String,
        client_id: String,
        surface_id: String,
        region: Option<ComputerSnapshotRegion>,
        max_width: Option<u32>,
        max_height: Option<u32>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if let Err(error) = validate_artifact_file_path(&path) {
            return ToolResult::err_with_output(
                error.clone(),
                json!({"error_kind": "artifact_policy", "project": project, "path": path, "message": bounded_text(&error)}),
            );
        }
        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(resolved) => resolved,
            Err(error) => return error.into_tool_result(),
        };
        if !resolved.config.is_agent() {
            return ToolResult::err("computer_save_snapshot requires an agent-registered project");
        }
        let target_client_id = match resolved.config.agent_client_id() {
            Ok(client_id) => client_id.to_string(),
            Err(error) => return ToolResult::err(error),
        };
        let target_cwd = resolved.config.path.clone();
        let project_id = resolved.resolved_id;
        let expected_project_prefix = format!("agent:{target_client_id}:");
        let target_agent_project_id = match project_id.strip_prefix(&expected_project_prefix) {
            Some(project_id) if !project_id.is_empty() => project_id.to_string(),
            _ => {
                return ToolResult::err(
                    "computer_save_snapshot resolved target project identity is invalid",
                )
            }
        };

        let capture = self
            .capture_computer_snapshot(&client_id, &surface_id, region, max_width, max_height, auth)
            .await;
        if !capture.success {
            return capture;
        }
        let snapshot = capture.output;
        let content_base64 = match snapshot.get("content_base64").and_then(Value::as_str) {
            Some(content) => content.to_string(),
            None => {
                return computer_error(
                    "invalid_runner_response",
                    "validated snapshot content is missing",
                )
            }
        };
        let decoded = match general_purpose::STANDARD.decode(content_base64.as_bytes()) {
            Ok(bytes) if !bytes.is_empty() && bytes.len() <= MAX_MCP_IMAGE_BYTES => bytes,
            _ => {
                return computer_error(
                    "invalid_runner_response",
                    "validated snapshot content is inconsistent",
                )
            }
        };
        let mime_type = match snapshot.get("mime_type").and_then(Value::as_str) {
            Some(mime) => mime.to_string(),
            None => {
                return computer_error(
                    "invalid_runner_response",
                    "validated snapshot MIME is missing",
                )
            }
        };
        if let Err(error) = validate_artifact_mime_for_path(&path, Some(&mime_type)) {
            return ToolResult::err_with_output(
                error.clone(),
                json!({"error_kind": "artifact_policy", "project": project_id, "path": path, "message": bounded_text(&error)}),
            );
        }
        let file_bytes = decoded.len();
        let sha256 = snapshot
            .get("sha256")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| sha256_hex(&decoded));
        let Some(surface) = snapshot.get("surface") else {
            return computer_error(
                "invalid_runner_response",
                "validated snapshot surface is missing",
            );
        };
        let source_width = snapshot
            .get("source_width")
            .and_then(Value::as_u64)
            .or_else(|| surface.get("width").and_then(Value::as_u64))
            .unwrap_or_default();
        let source_height = snapshot
            .get("source_height")
            .and_then(Value::as_u64)
            .or_else(|| surface.get("height").and_then(Value::as_u64))
            .unwrap_or_default();
        let width = snapshot
            .get("width")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let height = snapshot
            .get("height")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let captured_region = snapshot.get("region").cloned().unwrap_or_else(
            || json!({"x": 0, "y": 0, "width": source_width, "height": source_height}),
        );

        let payload = json!({
            "path": path.clone(),
            "content_base64": content_base64,
            "mime_type": mime_type,
            "overwrite": false,
            "max_bytes": MAX_MCP_IMAGE_BYTES,
        });
        let serialized = match serde_json::to_string(&payload) {
            Ok(serialized) => serialized,
            Err(_) => {
                return computer_error(
                    "invalid_request",
                    "could not encode snapshot artifact request",
                )
            }
        };
        let wait_timeout = 60_u64;
        let request = ShellFileOpRequest {
            op: "save_project_artifact".to_string(),
            client_id: target_client_id,
            path: path.clone(),
            cwd: Some(target_cwd.clone()),
            content: Some(serialized),
            max_bytes: None,
            old_text: None,
            pattern: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            line: None,
            create_dirs: false,
            wait_timeout_secs: wait_timeout,
        };
        let requested_by = crate::shell_client::requested_by_from_auth(auth);
        let (request_id, receiver) = match self
            .shell_clients
            .enqueue_computer_snapshot_artifact(
                request,
                &target_agent_project_id,
                &target_cwd,
                requested_by,
                auth,
            )
            .await
        {
            Ok(request) => request,
            Err(error) => {
                return computer_snapshot_artifact_lifecycle_failure(
                    &format!("snapshot artifact write was not dispatched: {error}"),
                    ShellCommandExecutionState::NotStarted,
                    &project_id,
                    &path,
                    &sha256,
                    file_bytes,
                    &mime_type,
                )
            }
        };
        let response = match tokio::time::timeout(Duration::from_secs(wait_timeout + 4), receiver)
            .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => {
                let state = dispatch_uncertainty_lifecycle(
                    self.shell_clients
                        .cancel_request_dispatch_state(&request_id)
                        .await,
                );
                return computer_snapshot_artifact_lifecycle_failure(
                    "snapshot artifact response channel closed before a terminal result was received",
                    state,
                    &project_id,
                    &path,
                    &sha256,
                    file_bytes,
                    &mime_type,
                );
            }
            Err(_) => {
                let state = dispatch_uncertainty_lifecycle(
                    self.shell_clients
                        .cancel_request_dispatch_state(&request_id)
                        .await,
                );
                return computer_snapshot_artifact_lifecycle_failure(
                    "timed out waiting for snapshot artifact write result",
                    state,
                    &project_id,
                    &path,
                    &sha256,
                    file_bytes,
                    &mime_type,
                );
            }
        };
        let state = agent_command_lifecycle(&response, wait_timeout);
        if state == ShellCommandExecutionState::NotStarted {
            return computer_snapshot_artifact_lifecycle_failure(
                "snapshot artifact write did not start",
                state,
                &project_id,
                &path,
                &sha256,
                file_bytes,
                &mime_type,
            );
        }
        if state != ShellCommandExecutionState::Completed {
            return computer_snapshot_artifact_lifecycle_failure(
                "snapshot artifact write did not return a definite terminal result",
                ShellCommandExecutionState::OutcomeUnknown,
                &project_id,
                &path,
                &sha256,
                file_bytes,
                &mime_type,
            );
        }
        if let Some(error) = response.error.as_deref() {
            return computer_snapshot_artifact_definite_failure(
                error,
                &project_id,
                &path,
                &sha256,
                file_bytes,
                &mime_type,
            );
        }
        if response.exit_code != Some(0) {
            return computer_snapshot_artifact_definite_failure(
                response
                    .stderr
                    .as_deref()
                    .unwrap_or("snapshot artifact write failed"),
                &project_id,
                &path,
                &sha256,
                file_bytes,
                &mime_type,
            );
        }
        let output: Value = match response
            .stdout
            .as_deref()
            .map(str::trim)
            .map(serde_json::from_str)
            .transpose()
        {
            Ok(Some(output)) => output,
            _ => {
                return computer_snapshot_artifact_lifecycle_failure(
                    "snapshot artifact write returned invalid JSON after possible commit",
                    ShellCommandExecutionState::OutcomeUnknown,
                    &project_id,
                    &path,
                    &sha256,
                    file_bytes,
                    &mime_type,
                )
            }
        };
        if let Some(error) = output.get("error").and_then(Value::as_str) {
            return computer_snapshot_artifact_definite_failure(
                error,
                &project_id,
                &path,
                &sha256,
                file_bytes,
                &mime_type,
            );
        }
        let metadata_matches = output.get("path").and_then(Value::as_str) == Some(path.as_str())
            && output.get("bytes_written").and_then(Value::as_u64) == Some(file_bytes as u64)
            && output.get("sha256").and_then(Value::as_str) == Some(sha256.as_str())
            && output.get("mime_type").and_then(Value::as_str) == Some(mime_type.as_str());
        if !metadata_matches {
            return computer_snapshot_artifact_lifecycle_failure(
                "snapshot artifact write returned inconsistent success metadata after possible commit",
                ShellCommandExecutionState::OutcomeUnknown,
                &project_id,
                &path,
                &sha256,
                file_bytes,
                &mime_type,
            );
        }

        ToolResult::ok(json!({
            "project": project_id,
            "path": path,
            "client_id": client_id,
            "surface_id": surface_id,
            "source_width": source_width,
            "source_height": source_height,
            "region": captured_region,
            "width": width,
            "height": height,
            "mime_type": mime_type,
            "file_bytes": file_bytes,
            "sha256": sha256,
            "saved": true,
        }))
    }

    async fn computer_list_targets(&self, auth: Option<&AuthContext>) -> ToolResult {
        let clients = self.shell_clients.list_clients_for_auth(auth).await;
        let mut total_count = 0usize;
        let mut targets = Vec::new();
        for client in clients {
            let computer_observe = client.capabilities.computer_observe;
            let computer_application_discovery = client.capabilities.computer_application_discovery;
            let computer_application_launch = client.capabilities.computer_application_launch;
            let computer_display_observe = client.capabilities.computer_display_observe;
            let computer_pointer_control = client.capabilities.computer_pointer_control;
            let computer_clipboard_read = client.capabilities.computer_clipboard_read;
            let computer_clipboard_write = client.capabilities.computer_clipboard_write;
            let computer_snapshot_region = client.capabilities.computer_snapshot_region;
            let computer_accessibility_observe = client.capabilities.computer_accessibility_observe;
            if !computer_observe
                && !computer_accessibility_observe
                && !computer_application_discovery
                && !computer_application_launch
                && !computer_display_observe
                && !computer_pointer_control
                && !computer_clipboard_read
                && !computer_clipboard_write
            {
                continue;
            }
            total_count = total_count.saturating_add(1);
            if targets.len() >= MAX_COMPUTER_TARGETS {
                continue;
            }
            targets.push(json!({
                "client_id": client.client_id,
                "display_name": client.display_name,
                "connected": client.connected,
                "capabilities": {
                    "computer_observe": computer_observe,
                    "computer_application_discovery": computer_application_discovery,
                    "computer_application_launch": computer_application_launch,
                    "computer_display_observe": computer_display_observe,
                    "computer_pointer_control": computer_pointer_control,
                    "computer_clipboard_read": computer_clipboard_read,
                    "computer_clipboard_write": computer_clipboard_write,
                    "computer_snapshot_region": computer_snapshot_region,
                    "computer_accessibility_observe": computer_accessibility_observe,
                },
            }));
        }
        let count = targets.len();
        ToolResult::ok(json!({
            "targets": targets,
            "count": count,
            "total_count": total_count,
            "truncated": total_count > count,
        }))
    }

    async fn dispatch_computer_request(
        &self,
        client_id: &str,
        kind: &'static str,
        payload: Value,
        auth: Option<&AuthContext>,
        list_limit: Option<usize>,
        expected_surface_id: Option<&str>,
        accessibility_bounds: Option<(usize, usize)>,
    ) -> ToolResult {
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
        let required_capabilities: &[&str] = match kind {
            "computer_list_applications" => {
                &[SHELL_CLIENT_CAPABILITY_COMPUTER_APPLICATION_DISCOVERY]
            }
            "computer_launch_application" => &[SHELL_CLIENT_CAPABILITY_COMPUTER_APPLICATION_LAUNCH],
            "computer_list_displays" | "computer_snapshot_display" => {
                &[SHELL_CLIENT_CAPABILITY_COMPUTER_DISPLAY_OBSERVE]
            }
            "computer_read_clipboard" => &[SHELL_CLIENT_CAPABILITY_COMPUTER_CLIPBOARD_READ],
            "computer_write_clipboard" => &[SHELL_CLIENT_CAPABILITY_COMPUTER_CLIPBOARD_WRITE],
            "computer_pointer_move" | "computer_pointer_click" => {
                &[SHELL_CLIENT_CAPABILITY_COMPUTER_POINTER_CONTROL]
            }
            "computer_list_windows" | "computer_snapshot" => {
                &[SHELL_CLIENT_CAPABILITY_COMPUTER_OBSERVE]
            }
            "computer_snapshot_region" => &[
                SHELL_CLIENT_CAPABILITY_COMPUTER_OBSERVE,
                SHELL_CLIENT_CAPABILITY_COMPUTER_SNAPSHOT_REGION,
            ],
            "computer_accessibility_status" | "computer_accessibility_tree" => {
                &[crate::shell_protocol::SHELL_CLIENT_CAPABILITY_COMPUTER_ACCESSIBILITY_OBSERVE]
            }
            "computer_element_state" => &[SHELL_CLIENT_CAPABILITY_COMPUTER_ELEMENT_STATE],
            "computer_control" => &[SHELL_CLIENT_CAPABILITY_COMPUTER_CONTROL],
            "computer_scroll_to_element" => &[SHELL_CLIENT_CAPABILITY_COMPUTER_SCROLL_TO_ELEMENT],
            "computer_key_input" => &[SHELL_CLIENT_CAPABILITY_COMPUTER_KEY_INPUT],
            "computer_activate_window" => &[SHELL_CLIENT_CAPABILITY_COMPUTER_WINDOW_ACTIVATE],
            "computer_input_text" => &[SHELL_CLIENT_CAPABILITY_COMPUTER_TEXT_INPUT],
            _ => return computer_error("invalid_request", "unsupported computer request kind"),
        };
        for required_capability in required_capabilities {
            match self
                .shell_clients
                .client_supports_for_auth(client_id, required_capability, auth)
                .await
            {
                Ok(true) => {}
                Ok(false) => {
                    if is_application_launch {
                        return computer_application_effect_not_started(
                            "capability_unavailable",
                            &format!("target Runner does not support {required_capability}"),
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
                Err(_error) if is_application_launch => {
                    return computer_application_effect_not_started(
                        "client_access_denied",
                        "caller cannot access the target Runner for application launch",
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
                Err(error) => return computer_error("client_access_denied", &error),
            }
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
        let requested_by = crate::shell_client::requested_by_from_auth(auth);
        let (request_id, receiver) = match self
            .shell_clients
            .enqueue_computer(
                client_id.to_string(),
                kind,
                payload,
                requested_by,
                auth,
                COMPUTER_WAIT_SECS,
            )
            .await
        {
            Ok(value) => value,
            Err(error) if kind == "computer_launch_application" => {
                return computer_application_effect_not_started(
                    "not_started",
                    &format!("application launch request was not dispatched: {error}"),
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
            Err(error) => return computer_error("dispatch_denied", &error),
        };
        let is_effect = computer_request_is_effect(kind);
        let is_text_input = kind == "computer_input_text";
        let response = match tokio::time::timeout(
            Duration::from_secs(COMPUTER_WAIT_SECS + 2),
            receiver,
        )
        .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(_)) if is_effect => {
                let request_dispatched = self
                    .shell_clients
                    .cancel_request_dispatch_state(&request_id)
                    .await;
                if is_application_launch {
                    return computer_application_effect_delivery_failure(
                        "Runner response channel closed before a terminal application launch result was received",
                        request_dispatched,
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
                    .shell_clients
                    .cancel_request_dispatch_state(&request_id)
                    .await;
                if is_application_launch {
                    return computer_application_effect_delivery_failure(
                        "Runner did not return a terminal application launch result in time",
                        request_dispatched,
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
                return computer_text_input_runner_error(error, response.request_dispatched);
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
                    expected_application_id.as_deref().unwrap_or_default(),
                );
            }
            if is_effect && error_kind == "outcome_unknown" {
                return computer_effect_outcome_unknown(error);
            }
            if is_effect && error_kind == "runner_error" {
                return computer_effect_delivery_failure(error, response.request_dispatched);
            }
            return computer_error(
                error_kind,
                &computer_error_recovery_message(error_kind, error),
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
        let output: Value = match response
            .stdout
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
        {
            Ok(Some(output)) => output,
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
            _ => {
                return computer_error(
                    "invalid_runner_response",
                    "Runner returned invalid computer JSON",
                )
            }
        };
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
}

fn computer_snapshot_artifact_lifecycle_failure(
    message: &str,
    state: ShellCommandExecutionState,
    project: &str,
    path: &str,
    sha256: &str,
    file_bytes: usize,
    mime_type: &str,
) -> ToolResult {
    if state == ShellCommandExecutionState::NotStarted {
        return ToolResult::err_with_output(
            message.to_string(),
            json!({
                "error_kind": "not_started",
                "message": bounded_text(message),
                "execution_state": "not_started",
                "state_changed": false,
                "project": project,
                "path": path,
                "expected_sha256": sha256,
                "expected_file_bytes": file_bytes,
                "expected_mime_type": mime_type,
            }),
        );
    }
    let message = format!(
        "{message}; the create-only artifact may already exist. Read metadata for this exact project/path and compare SHA-256, byte count, and MIME before deciding whether another attempt is safe"
    );
    ToolResult::err_with_output(
        message.clone(),
        json!({
            "error_kind": "outcome_unknown",
            "message": bounded_text(&message),
            "execution_state": "outcome_unknown",
            "project": project,
            "path": path,
            "expected_sha256": sha256,
            "expected_file_bytes": file_bytes,
            "expected_mime_type": mime_type,
            "reconcile_with": "read_project_artifact_metadata",
        }),
    )
}

fn computer_snapshot_artifact_definite_failure(
    message: &str,
    project: &str,
    path: &str,
    sha256: &str,
    file_bytes: usize,
    mime_type: &str,
) -> ToolResult {
    ToolResult::err_with_output(
        message.to_string(),
        json!({
            "error_kind": "artifact_write_failed",
            "message": bounded_text(message),
            "execution_state": "completed",
            "state_changed": false,
            "project": project,
            "path": path,
            "expected_sha256": sha256,
            "expected_file_bytes": file_bytes,
            "expected_mime_type": mime_type,
        }),
    )
}

fn computer_error_recovery_message(error_kind: &str, error: &str) -> String {
    match error_kind {
        "stale_element" => format!(
            "{error}; reacquire a fresh element_id with computer_find_elements on the same surface"
        ),
        "stale_surface" => format!(
            "{error}; reacquire a fresh surface_id with computer_list_windows before continuing"
        ),
        "stale_application" => format!(
            "{error}; reacquire a fresh application_id with computer_list_applications before another launch"
        ),
        "stale_display" => format!(
            "{error}; reacquire a fresh display_id with computer_list_displays before continuing"
        ),
        _ => error.to_string(),
    }
}

fn computer_request_is_effect(kind: &str) -> bool {
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

fn node_matches_find_query(
    node: &Value,
    role: Option<&str>,
    subrole: Option<&str>,
    label: Option<&str>,
    focused: Option<bool>,
    enabled: Option<bool>,
) -> bool {
    if role.is_some_and(|expected| node.get("role").and_then(Value::as_str) != Some(expected)) {
        return false;
    }
    if subrole.is_some_and(|expected| node.get("subrole").and_then(Value::as_str) != Some(expected))
    {
        return false;
    }
    if label.is_some_and(|expected| {
        !["title", "description", "placeholder"]
            .into_iter()
            .filter_map(|field| node.get(field).and_then(Value::as_str))
            .any(|value| value.contains(expected))
    }) {
        return false;
    }
    if focused
        .is_some_and(|expected| node.get("focused").and_then(Value::as_bool) != Some(expected))
    {
        return false;
    }
    if enabled
        .is_some_and(|expected| node.get("enabled").and_then(Value::as_bool) != Some(expected))
    {
        return false;
    }
    true
}

fn filter_accessibility_tree(
    tree: Value,
    expected_surface_id: &str,
    role: Option<&str>,
    subrole: Option<&str>,
    label: Option<&str>,
    focused: Option<bool>,
    enabled: Option<bool>,
    limit: usize,
) -> ToolResult {
    let platform = tree.get("platform").cloned().unwrap_or(Value::Null);
    let nodes = match tree.get("nodes").and_then(Value::as_array) {
        Some(nodes) => nodes,
        None => {
            return computer_error(
                "invalid_runner_response",
                "validated Accessibility tree is missing nodes",
            )
        }
    };
    let source_truncated = tree
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let observation_generation = match tree.get("observation_generation").and_then(Value::as_u64) {
        Some(value) if value > 0 && value <= u32::MAX as u64 => value,
        _ => {
            return computer_error(
                "invalid_runner_response",
                "validated Accessibility tree is missing observation_generation",
            )
        }
    };
    let mut total_matches = 0usize;
    let mut elements = Vec::with_capacity(limit.min(nodes.len()));
    for node in nodes {
        if !node_matches_find_query(node, role, subrole, label, focused, enabled) {
            continue;
        }
        total_matches = total_matches.saturating_add(1);
        if elements.len() >= limit {
            continue;
        }
        elements.push(json!({
            "element_id": node.get("element_id").cloned().unwrap_or(Value::Null),
            "role": node.get("role").cloned().unwrap_or(Value::Null),
            "subrole": node.get("subrole").cloned().unwrap_or(Value::Null),
            "title": node.get("title").cloned().unwrap_or(Value::Null),
            "description": node.get("description").cloned().unwrap_or(Value::Null),
            "placeholder": node.get("placeholder").cloned().unwrap_or(Value::Null),
            "enabled": node.get("enabled").cloned().unwrap_or(Value::Null),
            "focused": node.get("focused").cloned().unwrap_or(Value::Null),
        }));
    }
    let count = elements.len();
    ToolResult::ok(json!({
        "platform": platform,
        "surface_id": expected_surface_id,
        "observation_generation": observation_generation,
        "elements": elements,
        "count": count,
        "scanned_nodes": nodes.len(),
        "truncated": source_truncated || total_matches > count,
    }))
}

fn computer_error(kind: &str, message: &str) -> ToolResult {
    ToolResult::err_with_output(
        message.to_string(),
        json!({"error_kind": kind, "message": bounded_text(message)}),
    )
}

#[derive(Clone, Debug)]
struct PointerRequestContext {
    display_id: String,
    snapshot_generation: u32,
    x: u32,
    y: u32,
}

fn computer_pointer_effect_not_started(
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
    ToolResult::err_with_output(message.to_string(), output)
}

fn computer_pointer_effect_spent_not_started(
    message: &str,
    context: &PointerRequestContext,
) -> ToolResult {
    let safe_message = format!(
        "{message}; snapshot_generation is spent. Reconcile with a fresh computer_snapshot_display observation before another pointer effect"
    );
    let mut result = computer_pointer_effect_not_started("not_started", &safe_message, context);
    result
        .output
        .as_object_mut()
        .expect("pointer spent not-started output is an object")
        .insert(
            "reconcile_with".to_string(),
            json!("computer_snapshot_display"),
        );
    result
}

fn computer_pointer_effect_outcome_unknown(
    message: &str,
    context: &PointerRequestContext,
) -> ToolResult {
    let safe_message = format!(
        "{message}; do not blindly retry. Reconcile with a fresh computer_snapshot_display observation first"
    );
    ToolResult::err_with_output(
        safe_message.clone(),
        json!({
            "error_kind": "outcome_unknown",
            "display_id": context.display_id,
            "snapshot_generation": context.snapshot_generation,
            "x": context.x,
            "y": context.y,
            "execution_state": "outcome_unknown",
            "reconcile_with": "computer_snapshot_display",
        }),
    )
}

fn computer_pointer_effect_delivery_failure(
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

fn computer_pointer_runner_error(
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
struct ClipboardWriteContext {
    text_bytes: Option<usize>,
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

fn computer_clipboard_write_not_started(
    error_kind: &str,
    message: &str,
    context: &ClipboardWriteContext,
) -> ToolResult {
    let mut output = clipboard_write_output_base(context);
    output.insert("error_kind".to_string(), json!(error_kind));
    output.insert("execution_state".to_string(), json!("not_started"));
    output.insert("state_changed".to_string(), json!(false));
    ToolResult::err_with_output(message.to_string(), Value::Object(output))
}

fn computer_clipboard_write_outcome_unknown(
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
}

fn computer_clipboard_write_delivery_failure(
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

fn computer_clipboard_write_runner_error(
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

fn computer_effect_not_started(message: &str) -> ToolResult {
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

fn computer_effect_outcome_unknown(message: &str) -> ToolResult {
    ToolResult::err_with_output(
        message.to_string(),
        json!({
            "error_kind": "outcome_unknown",
            "message": bounded_text(message),
            "execution_state": "outcome_unknown"
        }),
    )
}

fn computer_effect_delivery_failure(message: &str, request_dispatched: Option<bool>) -> ToolResult {
    if request_dispatched == Some(false) {
        computer_effect_not_started(message)
    } else {
        computer_effect_outcome_unknown(&format!(
            "{message}; the action may have taken effect, so inspect current UI state before retrying"
        ))
    }
}

fn computer_effect_validated_result(result: ToolResult, inconsistent_message: &str) -> ToolResult {
    if result.success {
        result
    } else {
        computer_effect_outcome_unknown(inconsistent_message)
    }
}

fn computer_application_effect_not_started(
    error_kind: &str,
    message: &str,
    application_id: &str,
) -> ToolResult {
    let application_id = valid_application_id(application_id).then(|| application_id.to_string());
    ToolResult::err_with_output(
        message.to_string(),
        json!({
            "error_kind": error_kind,
            "message": bounded_text(message),
            "application_id": application_id,
            "state_changed": false,
            "execution_state": "not_started",
        }),
    )
}

fn computer_application_effect_outcome_unknown(message: &str, application_id: &str) -> ToolResult {
    let safe_message = format!(
        "{message}; do not blindly retry. Reconcile with a fresh computer_list_windows observation first"
    );
    ToolResult::err_with_output(
        safe_message.clone(),
        json!({
            "error_kind": "outcome_unknown",
            "message": bounded_text(&safe_message),
            "application_id": application_id,
            "execution_state": "outcome_unknown",
            "reconcile_with": "computer_list_windows",
        }),
    )
}

fn computer_application_effect_delivery_failure(
    message: &str,
    request_dispatched: Option<bool>,
    application_id: &str,
) -> ToolResult {
    if request_dispatched == Some(false) {
        computer_application_effect_not_started("not_started", message, application_id)
    } else {
        computer_application_effect_outcome_unknown(message, application_id)
    }
}

fn computer_application_launch_runner_error(
    error: &str,
    request_dispatched: Option<bool>,
    application_id: &str,
) -> ToolResult {
    match classify_runner_error(error) {
        "stale_application" => computer_application_effect_not_started(
            "stale_application",
            "application_id is stale; run computer_list_applications again before another launch",
            application_id,
        ),
        "invalid_request" => computer_application_effect_not_started(
            "invalid_request",
            "Runner rejected the application launch request before native dispatch",
            application_id,
        ),
        "unsupported_platform" => computer_application_effect_not_started(
            "unsupported_platform",
            "application launch is unsupported by the target platform",
            application_id,
        ),
        "application_failed" => computer_application_effect_not_started(
            "application_failed",
            "Windows application identity could not be revalidated before native dispatch",
            application_id,
        ),
        "outcome_unknown" => computer_application_effect_outcome_unknown(
            "Runner reported an uncertain native application launch outcome",
            application_id,
        ),
        _ => computer_application_effect_delivery_failure(
            "Runner application launch ended without a recognized structured result",
            request_dispatched,
            application_id,
        ),
    }
}

fn computer_text_input_runner_error(error: &str, request_dispatched: Option<bool>) -> ToolResult {
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
        "stale_surface" => computer_error(error_kind, "Computer text input surface is stale"),
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

fn classify_runner_error(error: &str) -> &'static str {
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

fn bounded_text(value: &str) -> String {
    let mut end = value.len().min(MAX_TEXT_BYTES);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn validate_surface(value: &Value, expected_id: Option<&str>) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| "surface must be an object".to_string())?;
    let allowed = [
        "surface_id",
        "application",
        "title",
        "width",
        "height",
        "focused",
        "active",
    ];
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("surface contains unsupported fields".to_string());
    }
    let surface_id = value
        .get("surface_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "surface_id missing".to_string())?;
    if surface_id.is_empty()
        || surface_id.len() > MAX_SURFACE_ID_BYTES
        || expected_id.is_some_and(|expected| expected != surface_id)
    {
        return Err("surface_id mismatch or invalid".to_string());
    }
    for field in ["application", "title"] {
        let text = value
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{field} missing"))?;
        if text.len() > MAX_TEXT_BYTES {
            return Err(format!("{field} exceeds bound"));
        }
    }
    for field in ["width", "height"] {
        let dimension = value
            .get(field)
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("{field} missing"))?;
        if dimension == 0 || dimension > 32768 {
            return Err(format!("{field} is out of range"));
        }
    }
    for field in ["focused", "active"] {
        if !value
            .get(field)
            .is_some_and(|value| value.is_boolean() || value.is_null())
        {
            return Err(format!("{field} must be boolean or null"));
        }
    }
    Ok(())
}

fn validate_application_list(output: Value, limit: usize) -> ToolResult {
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "Runner application list is not an object",
            )
        }
    };
    let allowed = ["applications", "count", "truncated"];
    if object.len() != allowed.len() || object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return computer_error(
            "invalid_runner_response",
            "Runner application list fields are inconsistent",
        );
    }
    let applications = match output.get("applications").and_then(Value::as_array) {
        Some(applications)
            if applications.len() <= limit && applications.len() <= MAX_APPLICATIONS =>
        {
            applications
        }
        _ => {
            return computer_error(
                "invalid_runner_response",
                "Runner application list exceeds bound or is missing",
            )
        }
    };
    let mut seen = std::collections::HashSet::with_capacity(applications.len());
    for application in applications {
        let Some(entry) = application.as_object() else {
            return computer_error(
                "invalid_runner_response",
                "Runner application entry is not an object",
            );
        };
        let allowed_entry = ["application_id", "display_name"];
        if entry.len() != allowed_entry.len()
            || entry
                .keys()
                .any(|key| !allowed_entry.contains(&key.as_str()))
        {
            return computer_error(
                "invalid_runner_response",
                "Runner application entry fields are inconsistent",
            );
        }
        let application_id = application
            .get("application_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let display_name = application
            .get("display_name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !valid_application_id(application_id)
            || !seen.insert(application_id)
            || display_name.is_empty()
            || display_name.len() > MAX_TEXT_BYTES
            || display_name.contains('\0')
        {
            return computer_error(
                "invalid_runner_response",
                "Runner application entry is invalid",
            );
        }
    }
    if output.get("count").and_then(Value::as_u64) != Some(applications.len() as u64)
        || output.get("truncated").and_then(Value::as_bool).is_none()
    {
        return computer_error(
            "invalid_runner_response",
            "Runner application list metadata is inconsistent",
        );
    }
    ToolResult::ok(output)
}

fn validate_display_list(output: Value, limit: usize) -> ToolResult {
    let Some(object) = output.as_object() else {
        return computer_error(
            "invalid_runner_response",
            "Runner display list is not an object",
        );
    };
    let allowed = ["displays", "count", "truncated"];
    if object.len() != allowed.len() || object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return computer_error(
            "invalid_runner_response",
            "Runner display list fields are inconsistent",
        );
    }
    let Some(displays) = output.get("displays").and_then(Value::as_array) else {
        return computer_error("invalid_runner_response", "Runner display list is missing");
    };
    if displays.len() > limit || displays.len() > MAX_DISPLAYS {
        return computer_error(
            "invalid_runner_response",
            "Runner display list exceeds bound",
        );
    }
    let mut seen = std::collections::HashSet::with_capacity(displays.len());
    for display in displays {
        let Some(entry) = display.as_object() else {
            return computer_error(
                "invalid_runner_response",
                "Runner display entry is not an object",
            );
        };
        let allowed_entry = ["display_id", "width", "height", "primary"];
        if entry.len() != allowed_entry.len()
            || entry
                .keys()
                .any(|key| !allowed_entry.contains(&key.as_str()))
        {
            return computer_error(
                "invalid_runner_response",
                "Runner display entry fields are inconsistent",
            );
        }
        let display_id = display
            .get("display_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let width = display
            .get("width")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let height = display
            .get("height")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        if !valid_display_id(display_id)
            || !seen.insert(display_id)
            || width == 0
            || width > u32::MAX as u64
            || height == 0
            || height > u32::MAX as u64
            || display.get("primary").and_then(Value::as_bool).is_none()
        {
            return computer_error("invalid_runner_response", "Runner display entry is invalid");
        }
    }
    if output.get("count").and_then(Value::as_u64) != Some(displays.len() as u64)
        || output.get("truncated").and_then(Value::as_bool).is_none()
    {
        return computer_error(
            "invalid_runner_response",
            "Runner display list metadata is inconsistent",
        );
    }
    ToolResult::ok(output)
}

fn expected_display_snapshot_dimensions(
    source_width: u64,
    source_height: u64,
    max_width: Option<u64>,
    max_height: Option<u64>,
) -> (u64, u64) {
    let width_scale = max_width
        .map(|bound| bound as f64 / source_width as f64)
        .unwrap_or(1.0);
    let height_scale = max_height
        .map(|bound| bound as f64 / source_height as f64)
        .unwrap_or(1.0);
    let scale = 1.0f64.min(width_scale).min(height_scale);
    if scale < 1.0 {
        let width = ((source_width as f64 * scale).floor() as u64)
            .max(1)
            .min(max_width.unwrap_or(u64::MAX));
        let height = ((source_height as f64 * scale).floor() as u64)
            .max(1)
            .min(max_height.unwrap_or(u64::MAX));
        (width, height)
    } else {
        (source_width, source_height)
    }
}

fn validate_display_snapshot(
    mut output: Value,
    expected_display_id: &str,
    client_id: &str,
    expected_max_width: Option<u64>,
    expected_max_height: Option<u64>,
) -> ToolResult {
    let Some(object) = output.as_object() else {
        return computer_error(
            "invalid_runner_response",
            "Runner display snapshot is not an object",
        );
    };
    let allowed = [
        "display_id",
        "snapshot_generation",
        "source_width",
        "source_height",
        "width",
        "height",
        "mime_type",
        "file_bytes",
        "sha256",
        "captured_at_unix_ms",
        "content_base64",
    ];
    if object.len() != allowed.len() || object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return computer_error(
            "invalid_runner_response",
            "Runner display snapshot fields are inconsistent",
        );
    }
    if !valid_display_id(expected_display_id)
        || output.get("display_id").and_then(Value::as_str) != Some(expected_display_id)
    {
        return computer_error(
            "invalid_runner_response",
            "Runner display snapshot identity is inconsistent",
        );
    }
    let generation = output
        .get("snapshot_generation")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let source_width = output
        .get("source_width")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let source_height = output
        .get("source_height")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    if generation == 0
        || generation > u32::MAX as u64
        || source_width == 0
        || source_width > u32::MAX as u64
        || source_height == 0
        || source_height > u32::MAX as u64
        || source_width
            .checked_mul(source_height)
            .and_then(|pixels| pixels.checked_mul(4))
            .is_none_or(|bytes| bytes > MAX_RAW_CAPTURE_BYTES)
    {
        return computer_error(
            "invalid_runner_response",
            "Runner display snapshot source geometry is invalid",
        );
    }
    let width = output
        .get("width")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let height = output
        .get("height")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let expected_dimensions = expected_display_snapshot_dimensions(
        source_width,
        source_height,
        expected_max_width,
        expected_max_height,
    );
    if (width, height) != expected_dimensions
        || width == 0
        || height == 0
        || width > MAX_IMAGE_DIMENSION
        || height > MAX_IMAGE_DIMENSION
    {
        return computer_error(
            "invalid_runner_response",
            "Runner display snapshot dimensions are inconsistent",
        );
    }
    if output.get("mime_type").and_then(Value::as_str) != Some("image/jpeg") {
        return computer_error(
            "invalid_runner_response",
            "Runner display snapshot MIME is invalid",
        );
    }
    let Some(encoded) = output.get("content_base64").and_then(Value::as_str) else {
        return computer_error(
            "invalid_runner_response",
            "Runner display snapshot content is missing",
        );
    };
    let decoded = match general_purpose::STANDARD.decode(encoded) {
        Ok(decoded) if !decoded.is_empty() && decoded.len() <= MAX_MCP_IMAGE_BYTES => decoded,
        _ => {
            return computer_error(
                "image_too_large",
                "Runner display snapshot image is invalid or too large",
            )
        }
    };
    if sniff_mime(&decoded) != Some("image/jpeg")
        || output.get("file_bytes").and_then(Value::as_u64) != Some(decoded.len() as u64)
        || output.get("sha256").and_then(Value::as_str) != Some(sha256_hex(&decoded).as_str())
    {
        return computer_error(
            "invalid_runner_response",
            "Runner display snapshot image metadata is inconsistent",
        );
    }
    const MAX_SAFE_JSON_INTEGER: u64 = 9_007_199_254_740_991;
    if !matches!(
        output.get("captured_at_unix_ms").and_then(Value::as_u64),
        Some(value) if value > 0 && value <= MAX_SAFE_JSON_INTEGER
    ) {
        return computer_error(
            "invalid_runner_response",
            "Runner display snapshot timestamp is invalid",
        );
    }
    output
        .as_object_mut()
        .expect("display snapshot object shape checked above")
        .insert("client_id".to_string(), json!(client_id));
    ToolResult::ok(output)
}

fn validate_computer_read_clipboard(output: Value) -> ToolResult {
    let Some(object) = output.as_object() else {
        return computer_error(
            "invalid_runner_response",
            "clipboard read result is not an object",
        );
    };
    if output.get("platform").and_then(Value::as_str) != Some("windows") {
        return computer_error(
            "invalid_runner_response",
            "clipboard read platform is inconsistent",
        );
    }
    let Some(available) = output.get("available").and_then(Value::as_bool) else {
        return computer_error(
            "invalid_runner_response",
            "clipboard read availability is missing",
        );
    };
    let Some(text_bytes) = output.get("text_bytes").and_then(Value::as_u64) else {
        return computer_error(
            "invalid_runner_response",
            "clipboard read byte count is missing",
        );
    };
    if text_bytes > MAX_CLIPBOARD_TEXT_BYTES as u64 {
        return computer_error(
            "invalid_runner_response",
            "clipboard read byte count exceeds bound",
        );
    }
    if available {
        let allowed = ["platform", "available", "text", "text_bytes"];
        let Some(text) = output.get("text").and_then(Value::as_str) else {
            return computer_error(
                "invalid_runner_response",
                "available clipboard text is missing",
            );
        };
        if object.len() != allowed.len()
            || object.keys().any(|key| !allowed.contains(&key.as_str()))
            || text.len() != text_bytes as usize
            || text.len() > MAX_CLIPBOARD_TEXT_BYTES
            || text.contains('\0')
        {
            return computer_error(
                "invalid_runner_response",
                "clipboard read success metadata is inconsistent",
            );
        }
    } else {
        let allowed = ["platform", "available", "text_bytes"];
        if object.len() != allowed.len()
            || object.keys().any(|key| !allowed.contains(&key.as_str()))
            || text_bytes != 0
            || object.contains_key("text")
        {
            return computer_error(
                "invalid_runner_response",
                "unavailable clipboard result is inconsistent",
            );
        }
    }
    ToolResult::ok(output)
}

fn validate_computer_write_clipboard(output: Value, context: &ClipboardWriteContext) -> ToolResult {
    let Some(object) = output.as_object() else {
        return computer_error(
            "invalid_runner_response",
            "clipboard write result is not an object",
        );
    };
    let allowed = ["platform", "text_bytes", "success"];
    if object.len() != allowed.len()
        || object.keys().any(|key| !allowed.contains(&key.as_str()))
        || output.get("platform").and_then(Value::as_str) != Some("windows")
        || output.get("success").and_then(Value::as_bool) != Some(true)
        || output.get("text_bytes").and_then(Value::as_u64)
            != context.text_bytes.map(|value| value as u64)
    {
        return computer_error(
            "invalid_runner_response",
            "clipboard write success metadata is inconsistent",
        );
    }
    ToolResult::ok(output)
}

fn validate_computer_launch_application(
    output: Value,
    expected_application_id: &str,
) -> ToolResult {
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "application launch result is not an object",
            )
        }
    };
    let allowed = ["platform", "application_id", "success"];
    if object.len() != allowed.len()
        || object.keys().any(|key| !allowed.contains(&key.as_str()))
        || output.get("platform").and_then(Value::as_str) != Some("windows")
        || output.get("application_id").and_then(Value::as_str) != Some(expected_application_id)
        || output.get("success").and_then(Value::as_bool) != Some(true)
    {
        return computer_error(
            "invalid_runner_response",
            "application launch success metadata is inconsistent",
        );
    }
    ToolResult::ok(output)
}

fn validate_window_list(output: Value, limit: usize) -> ToolResult {
    let windows = match output.get("windows").and_then(Value::as_array) {
        Some(windows) if windows.len() <= limit && windows.len() <= MAX_WINDOWS => windows,
        _ => {
            return computer_error(
                "invalid_runner_response",
                "Runner window list exceeds bound or is missing",
            )
        }
    };
    if let Some(error) = windows
        .iter()
        .find_map(|surface| validate_surface(surface, None).err())
    {
        return computer_error("invalid_runner_response", &error);
    }
    let count = output.get("count").and_then(Value::as_u64);
    let truncated = output.get("truncated").and_then(Value::as_bool);
    if count != Some(windows.len() as u64) || truncated.is_none() {
        return computer_error(
            "invalid_runner_response",
            "Runner window list metadata is inconsistent",
        );
    }
    ToolResult::ok(output)
}

fn is_native_accessibility_platform(value: Option<&str>) -> bool {
    matches!(value, Some("macos" | "windows"))
}

fn validate_accessibility_status(output: Value) -> ToolResult {
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "Accessibility status is not an object",
            )
        }
    };
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "platform" | "trusted"))
        || !is_native_accessibility_platform(output.get("platform").and_then(Value::as_str))
        || output.get("trusted").and_then(Value::as_bool).is_none()
    {
        return computer_error(
            "invalid_runner_response",
            "Accessibility status is malformed",
        );
    }
    ToolResult::ok(output)
}

fn validate_accessibility_node(
    value: &Value,
    max_depth: usize,
    seen: &mut HashMap<String, usize>,
) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| "accessibility node must be an object".to_string())?;
    let allowed = [
        "element_id",
        "parent_element_id",
        "depth",
        "role",
        "subrole",
        "title",
        "description",
        "value",
        "placeholder",
        "enabled",
        "focused",
        "child_count",
    ];
    if object.len() != allowed.len() || object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("accessibility node fields are inconsistent".to_string());
    }
    let element_id = value
        .get("element_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "element_id missing".to_string())?;
    if !element_id.starts_with("element_")
        || element_id.len() > MAX_ELEMENT_ID_BYTES
        || element_id.len() <= "element_".len()
        || seen.contains_key(element_id)
    {
        return Err("element_id is invalid or duplicated".to_string());
    }
    let depth = value
        .get("depth")
        .and_then(Value::as_u64)
        .and_then(|depth| usize::try_from(depth).ok())
        .ok_or_else(|| "accessibility node depth missing".to_string())?;
    if depth > max_depth {
        return Err("accessibility node depth exceeds requested bound".to_string());
    }
    match value.get("parent_element_id") {
        Some(Value::Null) if depth == 0 => {}
        Some(parent) if depth > 0 => {
            let parent = parent
                .as_str()
                .ok_or_else(|| "parent_element_id must be string or null".to_string())?;
            if parent.len() > MAX_ELEMENT_ID_BYTES || seen.get(parent).copied() != Some(depth - 1) {
                return Err("parent_element_id is stale or structurally invalid".to_string());
            }
        }
        _ => return Err("root accessibility parent/depth is inconsistent".to_string()),
    }
    let role = value
        .get("role")
        .and_then(Value::as_str)
        .ok_or_else(|| "accessibility role missing".to_string())?;
    if role.is_empty() || role.len() > MAX_TEXT_BYTES {
        return Err("accessibility role exceeds bound or is empty".to_string());
    }
    for field in ["subrole", "title", "description", "value", "placeholder"] {
        match value.get(field) {
            Some(Value::Null) => {}
            Some(text)
                if text
                    .as_str()
                    .is_some_and(|text| text.len() <= MAX_TEXT_BYTES) => {}
            _ => {
                return Err(format!(
                    "accessibility {field} is malformed or exceeds bound"
                ))
            }
        }
    }
    for field in ["enabled", "focused"] {
        if !value
            .get(field)
            .is_some_and(|value| value.is_boolean() || value.is_null())
        {
            return Err(format!("accessibility {field} must be boolean or null"));
        }
    }
    let child_count = value
        .get("child_count")
        .and_then(Value::as_u64)
        .ok_or_else(|| "accessibility child_count missing".to_string())?;
    if child_count > MAX_ACCESSIBILITY_CHILD_COUNT {
        return Err("accessibility child_count exceeds bound".to_string());
    }
    seen.insert(element_id.to_string(), depth);
    Ok(())
}

fn validate_accessibility_tree(
    output: Value,
    expected_surface_id: &str,
    max_depth: usize,
    max_nodes: usize,
) -> ToolResult {
    if max_depth > MAX_ACCESSIBILITY_DEPTH || !(1..=MAX_ACCESSIBILITY_NODES).contains(&max_nodes) {
        return computer_error(
            "invalid_request",
            "Accessibility validation bounds are invalid",
        );
    }
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "Accessibility tree is not an object",
            )
        }
    };
    let allowed = [
        "platform",
        "surface_id",
        "nodes",
        "node_count",
        "truncated",
        "max_depth",
        "max_nodes",
        "observation_generation",
    ];
    if object.len() != allowed.len() || object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return computer_error(
            "invalid_runner_response",
            "Accessibility tree fields are inconsistent",
        );
    }
    if !is_native_accessibility_platform(output.get("platform").and_then(Value::as_str))
        || output.get("surface_id").and_then(Value::as_str) != Some(expected_surface_id)
        || output.get("max_depth").and_then(Value::as_u64) != Some(max_depth as u64)
        || output.get("max_nodes").and_then(Value::as_u64) != Some(max_nodes as u64)
        || output.get("truncated").and_then(Value::as_bool).is_none()
        || !output
            .get("observation_generation")
            .and_then(Value::as_u64)
            .is_some_and(|value| value > 0 && value <= u32::MAX as u64)
    {
        return computer_error(
            "invalid_runner_response",
            "Accessibility tree metadata is inconsistent",
        );
    }
    let nodes = match output.get("nodes").and_then(Value::as_array) {
        Some(nodes) if !nodes.is_empty() && nodes.len() <= max_nodes => nodes,
        _ => {
            return computer_error(
                "invalid_runner_response",
                "Accessibility node list is missing or exceeds bound",
            )
        }
    };
    if output.get("node_count").and_then(Value::as_u64) != Some(nodes.len() as u64) {
        return computer_error(
            "invalid_runner_response",
            "Accessibility node count is inconsistent",
        );
    }
    let mut seen = HashMap::with_capacity(nodes.len());
    if let Some(error) = nodes
        .iter()
        .find_map(|node| validate_accessibility_node(node, max_depth, &mut seen).err())
    {
        return computer_error("invalid_runner_response", &error);
    }
    ToolResult::ok(output)
}

fn validate_computer_element_state(
    output: Value,
    expected_surface_id: &str,
    expected_element_id: &str,
) -> ToolResult {
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "computer element state is not an object",
            )
        }
    };
    let allowed = [
        "platform",
        "surface_id",
        "element_id",
        "observation_generation",
        "enabled",
        "focused",
        "protected",
        "value_empty",
        "can_press",
        "can_focus",
        "can_input_text",
    ];
    let bool_or_null = |field: &str| {
        output
            .get(field)
            .is_some_and(|value| value.is_boolean() || value.is_null())
    };
    if object.len() != allowed.len()
        || object.keys().any(|key| !allowed.contains(&key.as_str()))
        || !is_native_accessibility_platform(output.get("platform").and_then(Value::as_str))
        || output.get("surface_id").and_then(Value::as_str) != Some(expected_surface_id)
        || output.get("element_id").and_then(Value::as_str) != Some(expected_element_id)
        || !output
            .get("observation_generation")
            .and_then(Value::as_u64)
            .is_some_and(|value| value > 0 && value <= u32::MAX as u64)
        || !bool_or_null("enabled")
        || !bool_or_null("focused")
        || output.get("protected").and_then(Value::as_bool).is_none()
        || !bool_or_null("value_empty")
        || output.get("can_press").and_then(Value::as_bool).is_none()
        || output.get("can_focus").and_then(Value::as_bool).is_none()
        || output
            .get("can_input_text")
            .and_then(Value::as_bool)
            .is_none()
        || (output.get("protected").and_then(Value::as_bool) == Some(true)
            && (output.get("value_empty") != Some(&Value::Null)
                || output.get("can_press").and_then(Value::as_bool) != Some(false)
                || output.get("can_focus").and_then(Value::as_bool) != Some(false)
                || output.get("can_input_text").and_then(Value::as_bool) != Some(false)))
        || (output.get("enabled").and_then(Value::as_bool) == Some(false)
            && (output.get("can_press").and_then(Value::as_bool) != Some(false)
                || output.get("can_focus").and_then(Value::as_bool) != Some(false)
                || output.get("can_input_text").and_then(Value::as_bool) != Some(false)))
        || (output.get("can_input_text").and_then(Value::as_bool) == Some(true)
            && (output.get("focused").and_then(Value::as_bool) != Some(true)
                || output.get("value_empty").and_then(Value::as_bool) != Some(true)))
    {
        return computer_error(
            "invalid_runner_response",
            "computer element state metadata is inconsistent",
        );
    }
    ToolResult::ok(output)
}

fn is_native_window_activation_platform(value: Option<&str>) -> bool {
    matches!(value, Some("macos" | "windows"))
}

fn validate_computer_activate_window(output: Value, expected_surface_id: &str) -> ToolResult {
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "computer window activation result is not an object",
            )
        }
    };
    let allowed = ["platform", "surface_id", "success"];
    if object.len() != allowed.len()
        || object.keys().any(|key| !allowed.contains(&key.as_str()))
        || !is_native_window_activation_platform(output.get("platform").and_then(Value::as_str))
        || output.get("surface_id").and_then(Value::as_str) != Some(expected_surface_id)
        || output.get("success").and_then(Value::as_bool) != Some(true)
    {
        return computer_error(
            "invalid_runner_response",
            "computer window activation result metadata is inconsistent",
        );
    }
    ToolResult::ok(output)
}

fn validate_computer_control(
    output: Value,
    expected_surface_id: &str,
    expected_element_id: &str,
    expected_action: &str,
) -> ToolResult {
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "computer control result is not an object",
            )
        }
    };
    let allowed = ["platform", "surface_id", "element_id", "action", "success"];
    if object.len() != allowed.len()
        || object.keys().any(|key| !allowed.contains(&key.as_str()))
        || !is_native_window_activation_platform(output.get("platform").and_then(Value::as_str))
        || output.get("surface_id").and_then(Value::as_str) != Some(expected_surface_id)
        || output.get("element_id").and_then(Value::as_str) != Some(expected_element_id)
        || output.get("action").and_then(Value::as_str) != Some(expected_action)
        || output.get("success").and_then(Value::as_bool) != Some(true)
    {
        return computer_error(
            "invalid_runner_response",
            "computer control result is inconsistent",
        );
    }
    ToolResult::ok(output)
}

fn validate_computer_scroll_to_element(
    output: Value,
    expected_surface_id: &str,
    expected_element_id: &str,
) -> ToolResult {
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "computer scroll result is not an object",
            )
        }
    };
    let allowed = ["platform", "surface_id", "element_id", "success"];
    if object.len() != allowed.len()
        || object.keys().any(|key| !allowed.contains(&key.as_str()))
        || !is_native_accessibility_platform(output.get("platform").and_then(Value::as_str))
        || output.get("surface_id").and_then(Value::as_str) != Some(expected_surface_id)
        || output.get("element_id").and_then(Value::as_str) != Some(expected_element_id)
        || output.get("success").and_then(Value::as_bool) != Some(true)
    {
        return computer_error(
            "invalid_runner_response",
            "computer scroll result is inconsistent",
        );
    }
    ToolResult::ok(output)
}

fn validate_computer_pointer(mut output: Value, context: &PointerRequestContext) -> ToolResult {
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "computer pointer result is not an object",
            )
        }
    };
    let allowed = [
        "platform",
        "display_id",
        "snapshot_generation",
        "x",
        "y",
        "success",
    ];
    if object.len() != allowed.len()
        || object.keys().any(|key| !allowed.contains(&key.as_str()))
        || output.get("platform").and_then(Value::as_str) != Some("windows")
        || output.get("display_id").and_then(Value::as_str) != Some(context.display_id.as_str())
        || output.get("snapshot_generation").and_then(Value::as_u64)
            != Some(u64::from(context.snapshot_generation))
        || output.get("x").and_then(Value::as_u64) != Some(u64::from(context.x))
        || output.get("y").and_then(Value::as_u64) != Some(u64::from(context.y))
        || output.get("success").and_then(Value::as_bool) != Some(true)
    {
        return computer_error(
            "invalid_runner_response",
            "computer pointer result is inconsistent",
        );
    }
    let object = output
        .as_object_mut()
        .expect("computer pointer output was validated as an object");
    object.insert("execution_state".to_string(), json!("completed"));
    object.insert("state_changed".to_string(), json!(true));
    ToolResult::ok(output)
}

fn validate_computer_key_input(
    output: Value,
    expected_surface_id: &str,
    expected_key: &str,
    expected_modifiers: &Value,
) -> ToolResult {
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "computer key input result is not an object",
            )
        }
    };
    let allowed = ["platform", "surface_id", "key", "modifiers", "success"];
    if object.len() != allowed.len()
        || object.keys().any(|key| !allowed.contains(&key.as_str()))
        || !is_native_accessibility_platform(output.get("platform").and_then(Value::as_str))
        || output.get("surface_id").and_then(Value::as_str) != Some(expected_surface_id)
        || output.get("key").and_then(Value::as_str) != Some(expected_key)
        || output.get("modifiers") != Some(expected_modifiers)
        || output.get("success").and_then(Value::as_bool) != Some(true)
    {
        return computer_error(
            "invalid_runner_response",
            "computer key input result is inconsistent",
        );
    }
    ToolResult::ok(output)
}

fn validate_computer_input_text(
    output: Value,
    expected_surface_id: &str,
    expected_element_id: &str,
    expected_text_bytes: usize,
) -> ToolResult {
    let object = match output.as_object() {
        Some(object) => object,
        None => {
            return computer_error(
                "invalid_runner_response",
                "computer text input result is not an object",
            )
        }
    };
    let allowed = [
        "platform",
        "surface_id",
        "element_id",
        "text_bytes",
        "success",
    ];
    if object.len() != allowed.len()
        || object.keys().any(|key| !allowed.contains(&key.as_str()))
        || !is_native_window_activation_platform(output.get("platform").and_then(Value::as_str))
        || output.get("surface_id").and_then(Value::as_str) != Some(expected_surface_id)
        || output.get("element_id").and_then(Value::as_str) != Some(expected_element_id)
        || output.get("text_bytes").and_then(Value::as_u64) != Some(expected_text_bytes as u64)
        || output.get("success").and_then(Value::as_bool) != Some(true)
    {
        return computer_error(
            "invalid_runner_response",
            "computer text input result is inconsistent",
        );
    }
    ToolResult::ok(output)
}

fn sniff_mime(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if data.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

fn snapshot_region_values(region: &Value) -> Option<(u64, u64, u64, u64)> {
    let object = region.as_object()?;
    let allowed = ["x", "y", "width", "height"];
    if object.len() != allowed.len() || object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return None;
    }
    Some((
        region.get("x")?.as_u64()?,
        region.get("y")?.as_u64()?,
        region.get("width")?.as_u64()?,
        region.get("height")?.as_u64()?,
    ))
}

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn validate_snapshot(
    mut output: Value,
    expected_surface_id: &str,
    client_id: &str,
    advanced: bool,
    expected_region: Option<&Value>,
    expected_max_width: Option<u64>,
    expected_max_height: Option<u64>,
) -> ToolResult {
    let Some(object) = output.as_object() else {
        return computer_error(
            "invalid_runner_response",
            "Runner snapshot output is not an object",
        );
    };
    let metadata_fields = [
        "source_width",
        "source_height",
        "region",
        "sha256",
        "captured_at_unix_ms",
    ];
    let metadata_present = metadata_fields
        .iter()
        .any(|field| object.contains_key(*field));
    if advanced && !metadata_present {
        return computer_error(
            "invalid_runner_response",
            "Runner advanced snapshot metadata is missing",
        );
    }
    if metadata_present
        && metadata_fields
            .iter()
            .any(|field| !object.contains_key(*field))
    {
        return computer_error(
            "invalid_runner_response",
            "Runner snapshot metadata is incomplete",
        );
    }

    let surface = match output.get("surface") {
        Some(surface) => surface,
        None => {
            return computer_error(
                "invalid_runner_response",
                "Runner snapshot surface is missing",
            )
        }
    };
    if let Err(error) = validate_surface(surface, Some(expected_surface_id)) {
        return computer_error("invalid_runner_response", &error);
    }
    let surface_width = surface.get("width").and_then(Value::as_u64).unwrap_or(0);
    let surface_height = surface.get("height").and_then(Value::as_u64).unwrap_or(0);
    let width = output.get("width").and_then(Value::as_u64).unwrap_or(0);
    let height = output.get("height").and_then(Value::as_u64).unwrap_or(0);
    if width == 0 || height == 0 || width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION {
        return computer_error(
            "invalid_runner_response",
            "Runner snapshot dimensions exceed bound",
        );
    }
    if expected_max_width.is_some_and(|bound| width > bound)
        || expected_max_height.is_some_and(|bound| height > bound)
    {
        return computer_error(
            "invalid_runner_response",
            "Runner snapshot exceeds requested output dimensions",
        );
    }

    let mime = output
        .get("mime_type")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !matches!(mime, "image/png" | "image/jpeg" | "image/webp") {
        return computer_error(
            "invalid_runner_response",
            "Runner snapshot MIME is unsupported",
        );
    }
    let encoded = match output.get("content_base64").and_then(Value::as_str) {
        Some(encoded) => encoded,
        None => {
            return computer_error(
                "invalid_runner_response",
                "Runner snapshot content is missing",
            )
        }
    };
    let decoded = match general_purpose::STANDARD.decode(encoded) {
        Ok(decoded) => decoded,
        Err(_) => {
            return computer_error(
                "invalid_runner_response",
                "Runner snapshot base64 is invalid",
            )
        }
    };
    if decoded.is_empty() || decoded.len() > MAX_MCP_IMAGE_BYTES {
        return computer_error(
            "image_too_large",
            "Runner snapshot exceeds native MCP image bound",
        );
    }
    if sniff_mime(&decoded) != Some(mime) {
        return computer_error(
            "invalid_runner_response",
            "Runner snapshot MIME does not match image bytes",
        );
    }
    if output.get("file_bytes").and_then(Value::as_u64) != Some(decoded.len() as u64) {
        return computer_error(
            "invalid_runner_response",
            "Runner snapshot byte count is inconsistent",
        );
    }

    if metadata_present {
        if output.get("source_width").and_then(Value::as_u64) != Some(surface_width)
            || output.get("source_height").and_then(Value::as_u64) != Some(surface_height)
        {
            return computer_error(
                "invalid_runner_response",
                "Runner snapshot source dimensions are inconsistent",
            );
        }
        let actual_region = output.get("region").and_then(snapshot_region_values);
        let expected_region = expected_region
            .and_then(snapshot_region_values)
            .or_else(|| Some((0, 0, surface_width, surface_height)));
        let Some((x, y, region_width, region_height)) = actual_region else {
            return computer_error(
                "invalid_runner_response",
                "Runner snapshot region metadata is invalid",
            );
        };
        if region_width == 0
            || region_height == 0
            || x.checked_add(region_width)
                .is_none_or(|right| right > surface_width)
            || y.checked_add(region_height)
                .is_none_or(|bottom| bottom > surface_height)
            || actual_region != expected_region
        {
            return computer_error(
                "invalid_runner_response",
                "Runner snapshot region metadata is inconsistent",
            );
        }
        if output.get("sha256").and_then(Value::as_str) != Some(sha256_hex(&decoded).as_str()) {
            return computer_error(
                "invalid_runner_response",
                "Runner snapshot SHA-256 is inconsistent",
            );
        }
        const MAX_SAFE_JSON_INTEGER: u64 = 9_007_199_254_740_991;
        if !matches!(
            output.get("captured_at_unix_ms").and_then(Value::as_u64),
            Some(value) if value > 0 && value <= MAX_SAFE_JSON_INTEGER
        ) {
            return computer_error(
                "invalid_runner_response",
                "Runner snapshot capture timestamp is invalid",
            );
        }
    }

    let Some(object) = output.as_object_mut() else {
        unreachable!("snapshot object shape checked above")
    };
    object.insert("client_id".to_string(), json!(client_id));
    ToolResult::ok(output)
}

#[cfg(test)]
#[path = "computer_tools_tests.rs"]
mod tests;

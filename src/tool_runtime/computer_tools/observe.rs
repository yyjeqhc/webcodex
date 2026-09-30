//! Read-only Computer action routing and bounded target discovery.

use super::accessibility::filter_accessibility_tree;
use super::effects::computer_error;
use super::inputs::{effective_snapshot_dimension_bounds, valid_display_id};
use super::*;
impl ToolRuntime {
    pub(super) async fn dispatch_computer_observation(
        &self,
        call: ComputerObserveToolCall,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        match call {
            ComputerObserveToolCall::Targets => self.computer_list_targets(auth).await,
            ComputerObserveToolCall::Windows { client_id, limit } => {
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
            ComputerObserveToolCall::Displays { client_id, limit } => {
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
            ComputerObserveToolCall::Applications { client_id, limit } => {
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
            ComputerObserveToolCall::AccessibilityStatus { client_id } => {
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
            ComputerObserveToolCall::AccessibilityTree {
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
            ComputerObserveToolCall::FindElements {
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
            ComputerObserveToolCall::ElementState {
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
            ComputerObserveToolCall::ReadClipboard { client_id } => {
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
            ComputerObserveToolCall::SnapshotWindow {
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
            ComputerObserveToolCall::SnapshotDisplay {
                client_id,
                display_id,
                max_width,
                max_height,
            } => {
                if !valid_display_id(&display_id) {
                    return computer_error("invalid_display", "display_id is invalid");
                }
                let (max_width, max_height) =
                    match effective_snapshot_dimension_bounds(max_width, max_height) {
                        Ok(bounds) => bounds,
                        Err(()) => {
                            return computer_error(
                                "invalid_request",
                                "display snapshot output dimension bound is invalid",
                            )
                        }
                    };
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
        }
    }

    pub(super) async fn computer_list_targets(&self, auth: Option<&AuthContext>) -> ToolResult {
        let clients = self
            .runner_registry
            .list_runner_semantic_views_for_auth(
                crate::runner_http::runner_access_from_auth(auth).as_ref(),
            )
            .await;
        let mut total_count = 0usize;
        let mut targets = Vec::new();
        for client in clients {
            let computer_observe = client.supports(RunnerFeature::ComputerObserve);
            let computer_application_discovery =
                client.supports(RunnerFeature::ComputerApplicationDiscovery);
            let computer_application_launch =
                client.supports(RunnerFeature::ComputerApplicationLaunch);
            let computer_display_observe = client.supports(RunnerFeature::ComputerDisplayObserve);
            let computer_pointer_control = client.supports(RunnerFeature::ComputerPointerControl);
            let computer_clipboard_read = client.supports(RunnerFeature::ComputerClipboardRead);
            let computer_clipboard_write = client.supports(RunnerFeature::ComputerClipboardWrite);
            let computer_snapshot_region = client.supports(RunnerFeature::ComputerSnapshotRegion);
            let computer_accessibility_observe =
                client.supports(RunnerFeature::ComputerAccessibilityObserve);
            let view = client.view;
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
                "client_id": view.client_id,
                "display_name": view.display_name,
                "connected": view.connected,
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
}

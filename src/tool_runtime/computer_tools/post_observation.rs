//! One admitted action followed by one bounded read; never a replay or UI transaction.
use super::effects::computer_effect_not_started;
use super::*;
use crate::tool_runtime::tool_call::ComputerPostObservation;

impl ToolRuntime {
    pub(super) async fn dispatch_computer_control(
        &self,
        call: ComputerControlToolCall,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let Some(observe_after) = call.observe_after().cloned() else {
            return self
                .dispatch_computer_control_action(call, auth, None)
                .await;
        };
        let (client_id, surface_id) = match &call {
            ComputerControlToolCall::Press {
                client_id,
                surface_id,
                ..
            }
            | ComputerControlToolCall::InputText {
                client_id,
                surface_id,
                ..
            }
            | ComputerControlToolCall::Scroll {
                client_id,
                surface_id,
                ..
            } => (client_id.clone(), surface_id.clone()),
            _ => unreachable!("only press/input_text/scroll admit post-observation"),
        };
        if client_id.is_empty()
            || client_id.len() > 128
            || surface_id.is_empty()
            || surface_id.len() > MAX_SURFACE_ID_BYTES
        {
            return computer_effect_not_started("client_id or surface_id is invalid");
        }
        let element_id = match &call {
            ComputerControlToolCall::Press { element_id, .. }
            | ComputerControlToolCall::InputText { element_id, .. }
            | ComputerControlToolCall::Scroll { element_id, .. } => element_id,
            _ => unreachable!(),
        };
        if !element_id.starts_with("element_")
            || element_id.len() <= "element_".len()
            || element_id.len() > MAX_ELEMENT_ID_BYTES
        {
            return computer_effect_not_started("element_id is invalid");
        }
        if let ComputerControlToolCall::InputText { text, .. } = &call {
            if let Err(error) = super::inputs::validate_input_text(text) {
                return computer_effect_not_started(error);
            }
        }
        // The public gateway also governs these exact scopes. Keep internal
        // composition from becoming a scope bypass through another caller.
        if auth.is_some_and(|auth| {
            !auth.has_scope(SCOPE_COMPUTER_READ) || !auth.has_scope(SCOPE_COMPUTER_CONTROL)
        }) {
            return computer_effect_not_started(
                "post-observation requires computer:read and computer:control",
            );
        }
        let target = match self
            .runner_registry
            .get_runner_semantic_view_checked_for_auth(
                &client_id,
                crate::runner_http::runner_access_from_auth(auth).as_ref(),
            )
            .await
        {
            Ok(target) if target.supports(RunnerFeature::ComputerAccessibilityObserve) => target,
            Ok(_) => {
                return computer_effect_not_started(
                    "post-observation requires computer_accessibility_observe",
                )
            }
            Err(_) => {
                return computer_effect_not_started(
                    "post-observation target Runner is unavailable or inaccessible",
                )
            }
        };
        let action_features: &[RunnerFeature] = match &call {
            ComputerControlToolCall::Press { .. } => &[RunnerFeature::ComputerControl],
            ComputerControlToolCall::InputText { .. } => &[RunnerFeature::ComputerTextInput],
            ComputerControlToolCall::Scroll { .. } => {
                webcodex_core::computer_scroll_contract::COMPUTER_SCROLL_CAPABILITIES
            }
            _ => unreachable!(),
        };
        if action_features
            .iter()
            .any(|feature| !target.supports(*feature))
        {
            return computer_effect_not_started(
                "target Runner does not support the requested action",
            );
        }
        let instance = target.view.runner_instance_id;
        let mut action = self
            .dispatch_computer_control_action(call, auth, Some(&instance))
            .await;
        if !action.success {
            return action;
        }
        let ComputerPostObservation::AccessibilityTree {
            max_depth,
            max_nodes,
        } = observe_after;
        let depth = max_depth
            .unwrap_or(DEFAULT_ACCESSIBILITY_DEPTH)
            .min(MAX_ACCESSIBILITY_DEPTH);
        let nodes = max_nodes
            .unwrap_or(DEFAULT_ACCESSIBILITY_NODES)
            .clamp(1, MAX_ACCESSIBILITY_NODES);
        // No old element/root id is reused. This read produces a fresh generation
        // and may observe intermediate UI state; it is not an app-settled barrier.
        let observed = self
            .dispatch_computer_request_for_instance(
                &client_id,
                "computer_accessibility_tree",
                json!({"surface_id":surface_id,"max_depth":depth,"max_nodes":nodes}),
                auth,
                None,
                Some(&surface_id),
                Some((depth, nodes)),
                Some(&instance),
            )
            .await;
        action.output["post_observation"] =
            serde_json::to_value(observed).expect("ToolResult serializes");
        action
    }
}

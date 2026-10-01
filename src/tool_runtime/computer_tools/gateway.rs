//! Action-sensitive governance and the canonical Computer entry points.

use super::*;
pub(super) fn computer_observe_policy(
    call: &ComputerObserveToolCall,
) -> SpecializedOperationPolicy {
    use ComputerObserveToolCall::*;
    match call {
        Displays { .. } | SnapshotDisplay { .. } => SpecializedOperationPolicy::read_all(
            SpecializedSource::Computer,
            call.action_name(),
            &[SCOPE_COMPUTER_READ, SCOPE_COMPUTER_DISPLAY_READ],
        ),
        ReadClipboard { .. } => SpecializedOperationPolicy::read_all(
            SpecializedSource::Computer,
            call.action_name(),
            &[SCOPE_COMPUTER_READ, SCOPE_COMPUTER_CLIPBOARD_READ],
        ),
        Targets
        | Windows { .. }
        | Applications { .. }
        | AccessibilityStatus { .. }
        | AccessibilityTree { .. }
        | FindElements { .. }
        | ElementState { .. }
        | SnapshotWindow { .. } => SpecializedOperationPolicy::read(
            SpecializedSource::Computer,
            call.action_name(),
            SCOPE_COMPUTER_READ,
        ),
    }
}

pub(super) fn computer_control_policy(
    call: &ComputerControlToolCall,
) -> SpecializedOperationPolicy {
    use ComputerControlToolCall::*;
    match call {
        LaunchApplication { .. } => SpecializedOperationPolicy::consequential(
            SpecializedSource::Computer,
            call.action_name(),
            SCOPE_COMPUTER_LAUNCH,
            "computer_control",
        ),
        PointerMove { .. } | PointerClick { .. } => SpecializedOperationPolicy::consequential_all(
            SpecializedSource::Computer,
            call.action_name(),
            &[
                SCOPE_COMPUTER_READ,
                SCOPE_COMPUTER_DISPLAY_READ,
                SCOPE_COMPUTER_CONTROL,
                SCOPE_COMPUTER_POINTER_CONTROL,
            ],
            "computer_control",
        ),
        WriteClipboard { .. } => SpecializedOperationPolicy::consequential_all(
            SpecializedSource::Computer,
            call.action_name(),
            &[SCOPE_COMPUTER_CONTROL, SCOPE_COMPUTER_CLIPBOARD_WRITE],
            "computer_control",
        ),
        ActivateWindow { .. }
        | Press { .. }
        | Focus { .. }
        | ScrollToElement { .. }
        | Key { .. }
        | InputText { .. } => SpecializedOperationPolicy::consequential(
            SpecializedSource::Computer,
            call.action_name(),
            SCOPE_COMPUTER_CONTROL,
            "computer_control",
        ),
    }
}

fn computer_specialized_terminal(result: &ToolResult) -> (&str, Option<&str>) {
    let dispatch_certainty = result
        .output
        .get("execution_state")
        .and_then(Value::as_str)
        .unwrap_or(if result.success {
            "completed"
        } else {
            "not_started"
        });
    let failure_kind = result
        .output
        .get("failure_kind")
        .or_else(|| result.output.get("error_kind"))
        .and_then(Value::as_str);
    (dispatch_certainty, failure_kind)
}
impl ToolRuntime {
    pub(crate) async fn invoke_computer_observe_gateway(
        &self,
        call: ComputerObserveToolCall,
        recording_session_id: Option<&str>,
        auth: Option<&AuthContext>,
        transport: SessionTransport,
    ) -> Result<ToolResult, SpecializedGovernanceDenial> {
        let policy = computer_observe_policy(&call);
        let identity = json!({"action": call.action_name()});
        let permit = self
            .govern_specialized_invocation(
                "observe_computer",
                policy,
                transport,
                recording_session_id,
                auth,
                &identity,
            )
            .await?;
        let result = self
            .dispatch_computer_tool(ToolCall::ComputerObserve(call), auth)
            .await;
        let (dispatch_certainty, failure_kind) = computer_specialized_terminal(&result);
        self.finish_specialized_invocation(
            permit,
            result.success,
            dispatch_certainty,
            failure_kind,
        );
        Ok(result)
    }

    pub(crate) async fn invoke_computer_control_gateway(
        &self,
        call: ComputerControlToolCall,
        recording_session_id: Option<&str>,
        auth: Option<&AuthContext>,
        transport: SessionTransport,
    ) -> Result<ToolResult, SpecializedGovernanceDenial> {
        let policy = computer_control_policy(&call);
        let identity = json!({"action": call.action_name()});
        let permit = self
            .govern_specialized_invocation(
                "control_computer",
                policy,
                transport,
                recording_session_id,
                auth,
                &identity,
            )
            .await?;
        let result = self
            .dispatch_computer_tool(ToolCall::ComputerControl(call), auth)
            .await;
        let (dispatch_certainty, failure_kind) = computer_specialized_terminal(&result);
        self.finish_specialized_invocation(
            permit,
            result.success,
            dispatch_certainty,
            failure_kind,
        );
        Ok(result)
    }

    pub(in crate::tool_runtime) async fn dispatch_computer_tool(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        match call {
            ToolCall::ComputerObserve(call) => self.dispatch_computer_observation(call, auth).await,
            ToolCall::ComputerControl(call) => self.dispatch_computer_control(call, auth).await,
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
}

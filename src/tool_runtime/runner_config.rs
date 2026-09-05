use std::time::Duration;

use crate::auth::AuthContext;
use crate::runner_http::{requested_by_from_auth, runner_access_from_auth, RunnerFeature};
use crate::runner_protocol::{
    RunnerConfigAction, RunnerConfigExecutionState, RunnerConfigOperationRequest,
    RunnerConfigOperationResponse, RUNNER_CONFIG_RESPONSE_MAX_BYTES,
};
use serde_json::Value;

use super::{RecoveryKind, ToolCall, ToolResult, ToolRuntime};

const RUNNER_CONFIG_WAIT_SECS: u64 = 32;

fn response_value(response: &RunnerConfigOperationResponse) -> Value {
    serde_json::to_value(response).unwrap_or_else(|_| {
        serde_json::json!({
            "action": match response.action {
                RunnerConfigAction::Check => "check",
                RunnerConfigAction::Reload => "reload",
            },
            "execution_state": "outcome_unknown",
            "valid": null,
            "current_generation": null,
            "error_code": "invalid_runner_response",
            "error_field": null,
            "error_reason": null,
            "restart_required": false,
            "restart_required_fields": [],
        })
    })
}

fn config_response(
    action: RunnerConfigAction,
    state: RunnerConfigExecutionState,
    current_generation: Option<u64>,
    code: &str,
) -> RunnerConfigOperationResponse {
    RunnerConfigOperationResponse {
        action,
        execution_state: state,
        valid: None,
        current_generation,
        error_code: Some(code.to_string()),
        error_field: None,
        error_reason: None,
        restart_required: false,
        restart_required_fields: Vec::new(),
    }
}

fn config_failure(
    response: RunnerConfigOperationResponse,
    message: &'static str,
    recovery: RecoveryKind,
) -> ToolResult {
    ToolResult::err_with_output(message, response_value(&response)).with_recovery(recovery, None)
}

fn not_started(action: RunnerConfigAction, code: &str) -> ToolResult {
    config_failure(
        config_response(action, RunnerConfigExecutionState::NotStarted, None, code),
        "Runner config operation was not started",
        if code == "invalid_request" {
            RecoveryKind::FixInput
        } else {
            RecoveryKind::Reobserve
        },
    )
}

fn delivery_failure(action: RunnerConfigAction, dispatched: Option<bool>) -> ToolResult {
    if dispatched == Some(false) {
        return not_started(action, "runner_unavailable");
    }
    config_failure(
        config_response(
            action,
            RunnerConfigExecutionState::OutcomeUnknown,
            None,
            "outcome_unknown",
        ),
        "Runner config operation may have executed but no trustworthy terminal response was received",
        RecoveryKind::Reconcile,
    )
}

fn invalid_response(action: RunnerConfigAction, dispatched: Option<bool>) -> ToolResult {
    if dispatched == Some(false) {
        return not_started(action, "invalid_runner_response");
    }
    config_failure(
        config_response(
            action,
            RunnerConfigExecutionState::OutcomeUnknown,
            None,
            "invalid_runner_response",
        ),
        "Runner config operation returned an invalid terminal response; execution outcome is unknown",
        RecoveryKind::Reconcile,
    )
}

fn safe_pre_dispatch_code(error: &str) -> &'static str {
    if error.starts_with("runner_replaced:") {
        "runner_replaced"
    } else if error.starts_with("capability_unavailable:") {
        "capability_unavailable"
    } else {
        "runner_unavailable"
    }
}

fn valid_client_id(client_id: &str) -> bool {
    !client_id.trim().is_empty()
        && client_id.len() <= 128
        && !client_id.chars().any(char::is_control)
}

impl ToolRuntime {
    pub(crate) async fn dispatch_runner_config_tool(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let (action, client_id, expected_generation) = match call {
            ToolCall::RunnerConfigCheck { client_id } => {
                (RunnerConfigAction::Check, client_id, None)
            }
            ToolCall::RunnerConfigReload {
                client_id,
                expected_generation,
            } => (
                RunnerConfigAction::Reload,
                client_id,
                Some(expected_generation),
            ),
            _ => return ToolResult::err("unsupported Runner config tool"),
        };
        if !valid_client_id(&client_id)
            || expected_generation.is_some_and(|generation| generation == 0)
        {
            return not_started(action, "invalid_request");
        }

        let access = runner_access_from_auth(auth);
        let semantic = match self
            .runner_registry
            .get_runner_semantic_view_checked_for_auth(&client_id, access.as_ref())
            .await
        {
            Ok(semantic) => semantic,
            Err(_) => return not_started(action, "runner_unavailable"),
        };
        if !semantic.supports(RunnerFeature::RunnerConfigControl) {
            return not_started(action, "capability_unavailable");
        }
        let runner_instance_id = semantic.view.runner_instance_id.clone();
        if runner_instance_id.is_empty() {
            return not_started(action, "runner_replaced");
        }

        let operation = RunnerConfigOperationRequest {
            action,
            expected_generation,
        };
        if operation.validate().is_err() {
            return not_started(action, "invalid_request");
        }
        let requested_by = requested_by_from_auth(auth);
        let (request_id, receiver) = match self
            .runner_registry
            .enqueue_runner_config(
                &client_id,
                &runner_instance_id,
                operation,
                access.as_ref(),
                requested_by,
            )
            .await
        {
            Ok(request) => request,
            Err(error) => return not_started(action, safe_pre_dispatch_code(&error)),
        };

        let response = match tokio::time::timeout(
            Duration::from_secs(RUNNER_CONFIG_WAIT_SECS),
            receiver,
        )
        .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(_)) | Err(_) => {
                let dispatched = self
                    .runner_registry
                    .cancel_request_dispatch_state(&request_id)
                    .await;
                return delivery_failure(action, dispatched);
            }
        };

        if response.error.is_some() || response.exit_code != Some(0) {
            if response.request_dispatched == Some(false) {
                let code = response
                    .error
                    .as_deref()
                    .map(safe_pre_dispatch_code)
                    .unwrap_or("runner_unavailable");
                return not_started(action, code);
            }
            return delivery_failure(action, response.request_dispatched);
        }
        let Some(stdout) = response.stdout.as_deref() else {
            return invalid_response(action, response.request_dispatched);
        };
        if stdout.len() > RUNNER_CONFIG_RESPONSE_MAX_BYTES {
            return invalid_response(action, response.request_dispatched);
        }
        let parsed = match serde_json::from_str::<RunnerConfigOperationResponse>(stdout) {
            Ok(parsed) => parsed,
            Err(_) => return invalid_response(action, response.request_dispatched),
        };
        if parsed.action != action
            || parsed.validate().is_err()
            || parsed.execution_state == RunnerConfigExecutionState::OutcomeUnknown
        {
            return invalid_response(action, response.request_dispatched);
        }

        match action {
            RunnerConfigAction::Check => match parsed.execution_state {
                RunnerConfigExecutionState::Completed => ToolResult::ok(response_value(&parsed)),
                RunnerConfigExecutionState::NotStarted => config_failure(
                    parsed,
                    "Runner config check was not started",
                    RecoveryKind::Reobserve,
                ),
                RunnerConfigExecutionState::OutcomeUnknown => unreachable!(),
            },
            RunnerConfigAction::Reload => {
                let expected = expected_generation.expect("reload requires generation");
                match (parsed.execution_state, parsed.valid) {
                    (RunnerConfigExecutionState::Completed, Some(true))
                        if parsed.current_generation == expected.checked_add(1) =>
                    {
                        ToolResult::ok(response_value(&parsed))
                    }
                    (RunnerConfigExecutionState::Completed, Some(false))
                        if parsed.current_generation == Some(expected) =>
                    {
                        config_failure(
                            parsed,
                            "Runner config reload rejected the disk candidate; active config is unchanged",
                            RecoveryKind::FixInput,
                        )
                    }
                    (RunnerConfigExecutionState::NotStarted, None) => config_failure(
                        parsed,
                        "Runner config reload was not started",
                        RecoveryKind::Reobserve,
                    ),
                    _ => invalid_response(action, response.request_dispatched),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_delivery_certainty_never_downgrades_possible_execution_to_not_started() {
        let not_started = delivery_failure(RunnerConfigAction::Reload, Some(false));
        assert!(!not_started.success);
        assert_eq!(not_started.output["execution_state"], "not_started");
        assert_eq!(not_started.output["error_code"], "runner_unavailable");

        for dispatched in [Some(true), None] {
            let uncertain = delivery_failure(RunnerConfigAction::Reload, dispatched);
            assert!(!uncertain.success);
            assert_eq!(uncertain.output["execution_state"], "outcome_unknown");
            assert_eq!(uncertain.output["error_code"], "outcome_unknown");
            assert!(uncertain.output["current_generation"].is_null());
        }
    }
}

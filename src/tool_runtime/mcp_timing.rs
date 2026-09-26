use super::{
    return_timing::ToolReturnTimingPolicy, sessions::SessionTransport, ToolCall, ToolResult,
};
use crate::mcp_host::McpHostRuntimePolicy;

/// MCP contributes only a Host/adapter return-latency upper bound. Canonical
/// ToolRuntime combines it with any trusted internal orchestration bound before
/// structured execution resolves its own lifetime and handoff budget.
pub(super) fn return_timing_policy(
    transport: SessionTransport,
    policy: McpHostRuntimePolicy,
) -> ToolReturnTimingPolicy {
    if matches!(transport, SessionTransport::Mcp) {
        ToolReturnTimingPolicy::handoff_max_secs(policy.max_sync_wait_secs)
    } else {
        ToolReturnTimingPolicy::unconstrained()
    }
}

/// Job observation is a separate contract from structured-execution handoff.
/// Only MCP Host latency policy narrows an explicit observation wait.
pub(super) fn normalize_observation_call_timing(
    call: &mut ToolCall,
    transport: SessionTransport,
    policy: McpHostRuntimePolicy,
) {
    if !matches!(transport, SessionTransport::Mcp) {
        return;
    }
    match call {
        ToolCall::ObserveJobs { wait_secs, .. } | ToolCall::JobTail { wait_secs, .. } => {
            if let Some(wait_secs) = wait_secs.as_mut() {
                *wait_secs = (*wait_secs).min(policy.continuation_wait_secs);
            }
        }
        _ => {}
    }
}

pub(super) fn normalize_result_timing(
    result: &mut ToolResult,
    transport: SessionTransport,
    policy: McpHostRuntimePolicy,
) {
    if !matches!(transport, SessionTransport::Mcp) {
        return;
    }
    let Some(continuation) = result.output.get_mut("continuation") else {
        return;
    };
    if continuation["tool"].as_str() != Some("observe_jobs") {
        return;
    }
    if let Some(wait_secs) = continuation["arguments"]["wait_secs"].as_u64() {
        continuation["arguments"]["wait_secs"] =
            serde_json::json!(wait_secs.min(policy.continuation_wait_secs));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_host::{McpHostConfig, McpHostProfile};

    fn host_code_mode_policy() -> McpHostRuntimePolicy {
        McpHostConfig {
            profile: McpHostProfile::HostCodeMode,
            host_budget_secs: None,
        }
        .runtime_policy()
    }

    fn normalize_structured_for_transport(
        call: &mut ToolCall,
        transport: SessionTransport,
        policy: McpHostRuntimePolicy,
    ) {
        super::super::return_timing::normalize_structured_handoff(
            call,
            return_timing_policy(transport, policy),
        );
    }

    #[test]
    fn mcp_structured_execution_defaults_and_clamps_sync_wait() {
        let policy = host_code_mode_policy();
        let mut default_call = ToolCall::from_tool_name(
            "run_shell",
            serde_json::json!({"project":"demo","command":"true"}),
        )
        .unwrap();
        normalize_structured_for_transport(&mut default_call, SessionTransport::Mcp, policy);
        match default_call {
            ToolCall::RunShell { sync_wait_secs, .. } => assert_eq!(sync_wait_secs, Some(5)),
            _ => unreachable!(),
        }

        let mut explicit_call = ToolCall::from_tool_name(
            "cargo_test",
            serde_json::json!({"project":"demo","sync_wait_secs":55}),
        )
        .unwrap();
        normalize_structured_for_transport(&mut explicit_call, SessionTransport::Mcp, policy);
        match explicit_call {
            ToolCall::CargoTest { sync_wait_secs, .. } => assert_eq!(sync_wait_secs, Some(5)),
            _ => unreachable!(),
        }
    }

    #[test]
    fn direct_mcp_defaults_to_ten_and_clamps_legacy_override_to_fifty_five() {
        let policy = McpHostRuntimePolicy::default();
        let mut default_call = ToolCall::from_tool_name(
            "run_shell",
            serde_json::json!({"project":"demo","command":"true"}),
        )
        .unwrap();
        normalize_structured_for_transport(&mut default_call, SessionTransport::Mcp, policy);
        match default_call {
            ToolCall::RunShell { sync_wait_secs, .. } => assert_eq!(sync_wait_secs, Some(10)),
            _ => unreachable!(),
        }

        let mut explicit_call = ToolCall::from_tool_name(
            "cargo_check",
            serde_json::json!({"project":"demo","sync_wait_secs":100}),
        )
        .unwrap();
        normalize_structured_for_transport(&mut explicit_call, SessionTransport::Mcp, policy);
        match explicit_call {
            ToolCall::CargoCheck { sync_wait_secs, .. } => assert_eq!(sync_wait_secs, Some(55)),
            _ => unreachable!(),
        }
    }

    #[test]
    fn api_transport_preserves_canonical_timing_inputs() {
        let policy = host_code_mode_policy();
        let mut call = ToolCall::from_tool_name(
            "run_shell",
            serde_json::json!({"project":"demo","command":"true","sync_wait_secs":55}),
        )
        .unwrap();
        normalize_structured_for_transport(&mut call, SessionTransport::Api, policy);
        match call {
            ToolCall::RunShell { sync_wait_secs, .. } => assert_eq!(sync_wait_secs, Some(55)),
            _ => unreachable!(),
        }
    }

    #[test]
    fn result_normalization_does_not_add_missing_continuation() {
        let mut result = ToolResult::ok(serde_json::json!({"value": 1}));
        normalize_result_timing(&mut result, SessionTransport::Mcp, host_code_mode_policy());
        assert_eq!(result.output, serde_json::json!({"value": 1}));
    }

    #[test]
    fn failed_handoff_recovery_continuation_is_still_host_bounded() {
        let mut result = ToolResult::err_with_output(
            "handoff observation failed".to_string(),
            serde_json::json!({
                "execution_state": "outcome_unknown",
                "job_id": "job-123",
                "continuation": {
                    "tool": "observe_jobs",
                    "arguments": {
                        "items": [{
                            "job_id": "job-123",
                            "after_observation_token": "token-123"
                        }],
                        "wait_secs": 55,
                        "wake_on": "terminal"
                    }
                }
            }),
        );
        assert!(!result.success);
        normalize_result_timing(&mut result, SessionTransport::Mcp, host_code_mode_policy());
        assert_eq!(result.output["continuation"]["arguments"]["wait_secs"], 5);
        assert_eq!(
            result.output["continuation"]["arguments"]["items"][0]["job_id"],
            "job-123"
        );
        assert_eq!(result.output["execution_state"], "outcome_unknown");
    }

    #[test]
    fn generated_continuation_wait_is_transport_aware_without_changing_identity() {
        let mut result = ToolResult::ok(serde_json::json!({
            "continuation": {
                "tool": "observe_jobs",
                "arguments": {
                    "items": [{
                        "job_id": "job-123",
                        "after_observation_token": "token-123"
                    }],
                    "wait_secs": 55,
                    "wake_on": "terminal"
                }
            }
        }));
        normalize_result_timing(&mut result, SessionTransport::Mcp, host_code_mode_policy());
        assert_eq!(result.output["continuation"]["arguments"]["wait_secs"], 5);
        assert_eq!(
            result.output["continuation"]["arguments"]["items"][0]["job_id"],
            "job-123"
        );
        assert_eq!(
            result.output["continuation"]["arguments"]["items"][0]["after_observation_token"],
            "token-123"
        );

        let mut api = ToolResult::ok(result.output.clone());
        api.output["continuation"]["arguments"]["wait_secs"] = serde_json::json!(55);
        normalize_result_timing(&mut api, SessionTransport::Api, host_code_mode_policy());
        assert_eq!(api.output["continuation"]["arguments"]["wait_secs"], 55);
    }

    #[test]
    fn mcp_job_observation_waits_are_host_bounded() {
        let policy = host_code_mode_policy();
        let mut observe = ToolCall::from_tool_name(
            "observe_jobs",
            serde_json::json!({"items":[{"job_id":"job"}],"wait_secs":100}),
        )
        .unwrap();
        normalize_observation_call_timing(&mut observe, SessionTransport::Mcp, policy);
        match observe {
            ToolCall::ObserveJobs { wait_secs, .. } => assert_eq!(wait_secs, Some(5)),
            _ => unreachable!(),
        }

        let mut direct = ToolCall::from_tool_name(
            "observe_jobs",
            serde_json::json!({"items":[{"job_id":"job"}],"wait_secs":100}),
        )
        .unwrap();
        normalize_observation_call_timing(
            &mut direct,
            SessionTransport::Mcp,
            McpHostRuntimePolicy::default(),
        );
        match direct {
            ToolCall::ObserveJobs { wait_secs, .. } => assert_eq!(wait_secs, Some(55)),
            _ => unreachable!(),
        }

        let mut tail = ToolCall::from_tool_name(
            "job_tail",
            serde_json::json!({"job_id":"job","wait_secs":100}),
        )
        .unwrap();
        normalize_observation_call_timing(&mut tail, SessionTransport::Mcp, policy);
        match tail {
            ToolCall::JobTail { wait_secs, .. } => assert_eq!(wait_secs, Some(5)),
            _ => unreachable!(),
        }

        let mut api = ToolCall::from_tool_name(
            "observe_jobs",
            serde_json::json!({"items":[{"job_id":"job"}],"wait_secs":100}),
        )
        .unwrap();
        normalize_observation_call_timing(&mut api, SessionTransport::Api, policy);
        match api {
            ToolCall::ObserveJobs { wait_secs, .. } => assert_eq!(wait_secs, Some(100)),
            _ => unreachable!(),
        }
    }
}

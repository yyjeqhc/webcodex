use super::{sessions::SessionTransport, ToolCall, ToolResult};
use crate::mcp_host::McpHostRuntimePolicy;

pub(super) fn normalize_call_timing(
    call: &mut ToolCall,
    transport: SessionTransport,
    policy: McpHostRuntimePolicy,
) {
    if !matches!(transport, SessionTransport::Mcp) {
        return;
    }

    match call {
        ToolCall::RunProcess { sync_wait_secs, .. }
        | ToolCall::RunScript { sync_wait_secs, .. }
        | ToolCall::RunShell { sync_wait_secs, .. }
        | ToolCall::RunSkillResource { sync_wait_secs, .. }
        | ToolCall::CargoCheck { sync_wait_secs, .. }
        | ToolCall::CargoTest { sync_wait_secs, .. }
        | ToolCall::GoTest { sync_wait_secs, .. } => {
            normalize_sync_wait(sync_wait_secs, policy);
        }
        ToolCall::CargoFmt {
            check,
            sync_wait_secs,
            ..
        } if *check == Some(true) => {
            normalize_sync_wait(sync_wait_secs, policy);
        }
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
    if !matches!(transport, SessionTransport::Mcp) || !result.success {
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

fn normalize_sync_wait(sync_wait_secs: &mut Option<u64>, policy: McpHostRuntimePolicy) {
    *sync_wait_secs = Some(
        sync_wait_secs
            .unwrap_or(policy.initial_job_handoff_secs)
            .min(policy.max_sync_wait_secs),
    );
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

    #[test]
    fn mcp_structured_execution_defaults_and_clamps_sync_wait() {
        let policy = host_code_mode_policy();
        let mut default_call = ToolCall::from_tool_name(
            "run_shell",
            serde_json::json!({"project":"demo","command":"true"}),
        )
        .unwrap();
        normalize_call_timing(&mut default_call, SessionTransport::Mcp, policy);
        match default_call {
            ToolCall::RunShell { sync_wait_secs, .. } => assert_eq!(sync_wait_secs, Some(5)),
            _ => unreachable!(),
        }

        let mut explicit_call = ToolCall::from_tool_name(
            "cargo_test",
            serde_json::json!({"project":"demo","sync_wait_secs":55}),
        )
        .unwrap();
        normalize_call_timing(&mut explicit_call, SessionTransport::Mcp, policy);
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
        normalize_call_timing(&mut default_call, SessionTransport::Mcp, policy);
        match default_call {
            ToolCall::RunShell { sync_wait_secs, .. } => assert_eq!(sync_wait_secs, Some(10)),
            _ => unreachable!(),
        }

        let mut explicit_call = ToolCall::from_tool_name(
            "cargo_check",
            serde_json::json!({"project":"demo","sync_wait_secs":100}),
        )
        .unwrap();
        normalize_call_timing(&mut explicit_call, SessionTransport::Mcp, policy);
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
        normalize_call_timing(&mut call, SessionTransport::Api, policy);
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
        normalize_call_timing(&mut observe, SessionTransport::Mcp, policy);
        match observe {
            ToolCall::ObserveJobs { wait_secs, .. } => assert_eq!(wait_secs, Some(5)),
            _ => unreachable!(),
        }

        let mut direct = ToolCall::from_tool_name(
            "observe_jobs",
            serde_json::json!({"items":[{"job_id":"job"}],"wait_secs":100}),
        )
        .unwrap();
        normalize_call_timing(
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
        normalize_call_timing(&mut tail, SessionTransport::Mcp, policy);
        match tail {
            ToolCall::JobTail { wait_secs, .. } => assert_eq!(wait_secs, Some(5)),
            _ => unreachable!(),
        }

        let mut api = ToolCall::from_tool_name(
            "observe_jobs",
            serde_json::json!({"items":[{"job_id":"job"}],"wait_secs":100}),
        )
        .unwrap();
        normalize_call_timing(&mut api, SessionTransport::Api, policy);
        match api {
            ToolCall::ObserveJobs { wait_secs, .. } => assert_eq!(wait_secs, Some(100)),
            _ => unreachable!(),
        }
    }
}

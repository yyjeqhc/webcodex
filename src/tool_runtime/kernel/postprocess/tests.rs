use super::*;
use crate::tool_runtime::kernel::{HostFileImportTrust, ToolTransport};
use crate::tool_runtime::ToolCall;
use serde_json::json;

fn context(transport: ToolTransport) -> ToolCallContext<'static> {
    ToolCallContext {
        transport,
        session_id: None,
        auth: None,
        window: None,
        record_oauth_scope_denials: false,
        host_file_import_trust: HostFileImportTrust::default(),
    }
}

fn process_result() -> ToolResult {
    ToolResult::ok(json!({
        "duration_ms":1,"exit_code":0,"stdout_tail":"PRIVATE_OUTPUT","stderr_tail":"",
        "stdout_lines":1,"stderr_lines":0,"stdout_truncated":false,"stderr_truncated":false,
        "command_started":true,"command_completed":true,"command_ok":true,
        "failure_kind":null,"tool_failure":false,"purpose":"diagnostic",
        "process_summary":"PRIVATE_COMMAND","cwd":".","executor":"agent",
        "execution_source":"run_process","execution_state":"completed",
        "promoted_to_job":false,"terminal":true,"job_id":null,"job_status":null,
        "observation_token":null,"effective_timeout_secs":60,"sync_wait_secs":10,
        "async_handoff_available":true,"permission":{"status":"auto_approved"},
        "session_recorded":true,"session_event_id":"ledger-only"
    }))
}

fn plan() -> ModelFacingProjectionPlan {
    ModelFacingProjectionPlan::capture(
        &ToolCall::from_tool_name(
            "run_process",
            json!({"project":"agent:runner:project","executable":"tool","args":[]}),
        )
        .unwrap(),
    )
}

#[tokio::test]
async fn post_record_captures_canonical_audit_and_telemetry_before_model_compaction() {
    let runtime = ToolRuntime::new_for_tests();
    let recorder = ToolCallRecorderMetadata::default();
    let correlation = ToolCallCorrelation {
        resolved_project: Some("agent:runner:project".into()),
        recorder_gap_session_id: Some("wc_sess_1234567890abcdef".into()),
        ..Default::default()
    };
    let mut previous = None;
    for transport in [ToolTransport::Api, ToolTransport::Mcp] {
        let mut telemetry =
            ModelErgonomicsTimer::start_with_arguments("run_process", &json!({})).unwrap();
        let completed = PostRecordResponse {
            tool_name: "run_process",
            context: context(transport),
            capabilities: ToolProtocolCapabilities::default(),
            recorder: &recorder,
            correlation: &correlation,
            business_session_id: None,
            window_reply: None,
        }
        .finish(&runtime, process_result(), plan(), Some(&mut telemetry))
        .await;
        let audit = completed.canonical_audit_output.unwrap();
        assert_eq!(audit["execution_state"], "completed");
        assert_eq!(audit["command_ok"], true);
        assert!(!audit.to_string().contains("PRIVATE"));
        assert!(audit.get("workflow_recording_attention").is_none());
        let output = &completed.result.output;
        assert_eq!(output["stdout_tail"], "PRIVATE_OUTPUT");
        assert!(output.get("execution_state").is_none());
        assert!(output.get("permission").is_none());
        assert!(output.get("session_recorded").is_none());
        assert_eq!(
            output["workflow_recording_attention"]["candidate_session_id"],
            "wc_sess_1234567890abcdef"
        );
        let record = telemetry
            .finish()
            .record_for_tool_result(&completed.result)
            .unwrap();
        assert_eq!(record.execution_state.as_deref(), Some("completed"));
        if let Some(previous) = &previous {
            assert_eq!(output, previous);
        }
        previous = Some(completed.result.output);
    }
}

#[tokio::test]
async fn post_record_preserves_effect_uncertainty_and_retry_identity() {
    let runtime = ToolRuntime::new_for_tests();
    let recorder = ToolCallRecorderMetadata::default();
    let correlation = ToolCallCorrelation::default();
    for state in ["outcome_unknown", "timed_out", "not_started"] {
        let mut result = process_result();
        result.success = false;
        result.error = Some("execution uncertain".into());
        result.output["execution_state"] = json!(state);
        result.output["command_ok"] = json!(false);
        result.output["failure_kind"] = json!(state);
        result.output["job_id"] = json!("wc_job_exact-existing");
        result.output["direct_retry_safe"] = json!(false);
        let completed = PostRecordResponse {
            tool_name: "run_process",
            context: context(ToolTransport::Mcp),
            capabilities: ToolProtocolCapabilities::default(),
            recorder: &recorder,
            correlation: &correlation,
            business_session_id: None,
            window_reply: None,
        }
        .finish(&runtime, result, plan(), None)
        .await;
        assert!(!completed.result.success);
        assert_eq!(
            completed.result.error.as_deref(),
            Some("execution uncertain")
        );
        assert_eq!(completed.result.output["execution_state"], state);
        assert_eq!(completed.result.output["failure_kind"], state);
        assert_eq!(completed.result.output["job_id"], "wc_job_exact-existing");
        assert_eq!(completed.result.output["direct_retry_safe"], false);
        assert_eq!(
            completed.canonical_audit_output.unwrap()["execution_state"],
            state
        );
    }
}

#[tokio::test]
async fn post_record_omits_redundant_or_over_budget_gap_without_changing_business_result() {
    let runtime = ToolRuntime::new_for_tests();
    let recorder = ToolCallRecorderMetadata::default();
    let correlation = ToolCallCorrelation {
        resolved_project: Some("agent:runner:project".into()),
        recorder_gap_session_id: Some("wc_sess_1234567890abcdef".into()),
        ..Default::default()
    };
    for oversized in [false, true] {
        let payload = if oversized {
            "x".repeat(webcodex_workspace::file_read_range::MAX_SERIALIZED_OUTPUT_BYTES)
        } else {
            "business".into()
        };
        let result = ToolResult::ok(json!({"payload":payload}));
        let completed = PostRecordResponse {
            tool_name: "list_tools",
            context: context(ToolTransport::Mcp),
            capabilities: ToolProtocolCapabilities::default(),
            recorder: &recorder,
            correlation: &correlation,
            business_session_id: if oversized {
                None
            } else {
                correlation.recorder_gap_session_id.as_deref()
            },
            window_reply: None,
        }
        .finish(
            &runtime,
            result,
            ModelFacingProjectionPlan::capture(
                &ToolCall::from_tool_name("list_tools", json!({})).unwrap(),
            ),
            None,
        )
        .await;
        assert!(completed.result.success);
        assert_eq!(completed.result.output, json!({"payload":payload}));
        assert!(completed.canonical_audit_output.is_none());
    }
}

//! Runtime audit integration tests that combine canonical requests with audit projections.

use crate::ToolCallAuditProjection;
use serde_json::json;
use webcodex_tool_contracts::{ObserveJobsWakeOn, ToolCall};

#[cfg(feature = "experimental-code-mode")]
#[test]
fn code_mode_exec_parses_outer_authority_and_omits_source_from_audit() {
    const PRIVATE_SOURCE: &str = "const secret = 'NEVER_PERSIST_CODE_MODE_SOURCE'; text(secret);";
    let session_id = format!("wc_sess_{}", "1".repeat(32));
    let call = ToolCall::from_tool_name(
        "code_mode_exec",
        json!({
            "project": "agent:special:demo",
            "session_id": session_id,
            "source": PRIVATE_SOURCE,
            "timeout_ms": 7_500,
        }),
    )
    .unwrap();
    assert_eq!(call.tool_name(), "code_mode_exec");
    assert_eq!(call.project(), Some("agent:special:demo"));
    assert_eq!(call.session_id(), Some(session_id.as_str()));
    let audit = call.session_log_arguments();
    assert_eq!(audit["project"], "agent:special:demo");
    assert_eq!(audit["source_bytes"], PRIVATE_SOURCE.len());
    assert_eq!(audit["timeout_ms"], 7_500);
    assert!(!audit.to_string().contains("NEVER_PERSIST_CODE_MODE_SOURCE"));

    let result_audit = crate::tool_audit::session_log_result_for_tool(
        "code_mode_exec",
        &json!({
            "content": ["NEVER_PERSIST_CODE_MODE_CONTENT"],
            "message": "NEVER_PERSIST_CODE_MODE_ERROR_DETAIL",
            "failure_kind": "runtime_error",
            "stats": {
                "tool_calls": 3,
                "max_in_flight": 2,
                "duration_ms": 17,
                "returned_bytes": 31
            }
        }),
    );
    assert_eq!(result_audit["failure_kind"], "runtime_error");
    assert_eq!(result_audit["tool_calls"], 3);
    assert_eq!(result_audit["max_in_flight"], 2);
    assert_eq!(result_audit["duration_ms"], 17);
    assert_eq!(result_audit["returned_bytes"], 31);
    assert!(result_audit.get("content").is_none());
    assert!(result_audit.get("message").is_none());
    let result_audit_text = result_audit.to_string();
    assert!(!result_audit_text.contains("NEVER_PERSIST_CODE_MODE_CONTENT"));
    assert!(!result_audit_text.contains("NEVER_PERSIST_CODE_MODE_ERROR_DETAIL"));
}

#[cfg(feature = "experimental-code-mode")]
#[test]
fn code_mode_exec_effectful_parses_outer_authority_and_omits_source_from_audit() {
    const PRIVATE_SOURCE: &str =
        "const secret = 'NEVER_PERSIST_EFFECTFUL_CODE_MODE_SOURCE'; text(secret);";
    let session_id = format!("wc_sess_{}", "2".repeat(32));
    let call = ToolCall::from_tool_name(
        "code_mode_exec_effectful",
        json!({
            "project": "agent:special:demo",
            "session_id": session_id,
            "source": PRIVATE_SOURCE,
            "timeout_ms": 4_000,
        }),
    )
    .unwrap();
    assert_eq!(call.tool_name(), "code_mode_exec_effectful");
    assert_eq!(call.project(), Some("agent:special:demo"));
    assert_eq!(call.session_id(), Some(session_id.as_str()));
    let audit = call.session_log_arguments();
    assert_eq!(audit["project"], "agent:special:demo");
    assert_eq!(audit["source_bytes"], PRIVATE_SOURCE.len());
    assert_eq!(audit["timeout_ms"], 4_000);
    assert!(!audit
        .to_string()
        .contains("NEVER_PERSIST_EFFECTFUL_CODE_MODE_SOURCE"));

    let result_audit = crate::tool_audit::session_log_result_for_tool(
        "code_mode_exec_effectful",
        &json!({
            "failure_kind": "timeout",
            "message": "PRIVATE_EFFECTFUL_FRONTEND_DETAIL",
            "effect_receipt": {
                "consequential_calls": 2,
                "known_results": 0,
                "job_handoffs": 2,
                "outcome_unknown": 0,
                "children": [{
                    "ordinal": 1,
                    "tool": "cargo_check",
                    "outcome": "job_handoff",
                    "job_id": "PRIVATE_JOB_ID",
                    "continuation": {"follow_up_kind": "fallback_recovery", "tool": "observe_jobs", "arguments": {"items": []}}
                }]
            }
        }),
    );
    assert_eq!(result_audit["failure_kind"], "timeout");
    assert_eq!(result_audit["consequential_calls"], 2);
    assert_eq!(result_audit["job_handoffs"], 2);
    let audit_text = result_audit.to_string();
    assert!(!audit_text.contains("PRIVATE_JOB_ID"));
    assert!(!audit_text.contains("PRIVATE_EFFECTFUL_FRONTEND_DETAIL"));
    assert!(result_audit.get("children").is_none());
}

#[test]
fn observe_session_messages_tool_call_and_audit_are_bounded() {
    let raw_token = "wsm2_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let call = ToolCall::from_tool_name(
        "observe_session_messages",
        json!({
            "session_id": "wc_sess_demo",
            "after_observation_token": raw_token,
            "wait_secs": 7,
            "limit": 25
        }),
    )
    .unwrap();
    match &call {
        ToolCall::ObserveSessionMessages {
            session_id,
            after_observation_token,
            wait_secs,
            limit,
        } => {
            assert_eq!(session_id, "wc_sess_demo");
            assert_eq!(after_observation_token.as_deref(), Some(raw_token));
            assert_eq!(*wait_secs, Some(7));
            assert_eq!(*limit, Some(25));
        }
        other => panic!("expected ObserveSessionMessages, got {other:?}"),
    }
    assert_eq!(call.tool_name(), "observe_session_messages");
    assert_eq!(
        call.session_log_arguments(),
        json!({
            "session_id": "wc_sess_demo",
            "wait_secs": 7,
            "limit": 25,
            "token_present": true
        })
    );
    let output_audit = crate::tool_audit::session_log_result_for_tool(
        "observe_session_messages",
        &json!({
            "success": true,
            "session_id": "wc_sess_demo",
            "messages": [{"message": "secret body"}],
            "observation_token": raw_token,
            "changed": true,
            "history_lost": false,
            "has_more": true,
            "wait_outcome": "immediate"
        }),
    );
    assert_eq!(output_audit["message_count"], 1);
    assert_eq!(output_audit["changed"], true);
    assert_eq!(output_audit["has_more"], true);
    assert!(output_audit.get("messages").is_none());
    assert!(output_audit.get("observation_token").is_none());
    assert!(!output_audit.to_string().contains("secret body"));
    assert!(!output_audit.to_string().contains(raw_token));

    let oversized = ToolCall::from_tool_name(
        "observe_session_messages",
        json!({
            "session_id": "wc_sess_demo",
            "after_observation_token": "x".repeat(webcodex_core::job_observation::MAX_JOB_OBSERVATION_TOKEN_LEN + 1)
        }),
    );
    assert!(oversized.is_err());
}

#[test]
fn start_coding_task_uses_generic_unknown_tool_and_privacy_paths() {
    let error = ToolCall::from_tool_name("start_coding_task", json!({"project": "demo"}))
        .expect_err("retired start_coding_task must be an ordinary unknown tool");
    assert!(
        error.contains("unknown tool 'start_coding_task'"),
        "{error}"
    );

    let audit = crate::tool_audit::session_log_arguments_for_tool_request(
        "start_coding_task",
        &json!({
            "project": "agent:legacy:demo",
            "path": "/private/legacy/path",
            "prompt": "PRIVATE_PROMPT",
            "secret": "PRIVATE_SECRET"
        }),
    );
    assert_eq!(audit, json!({}));
}

#[test]
fn agent_continuation_bind_parses_required_view_fence_and_omits_it_from_audit() {
    let binding_id = format!("wc_host_binding_{}", "a0".repeat(16));
    let mut args = json!({
        "agent_id": "wc_dagent_qqqqqqqqqqqqqqqq".to_string(),
        "endpoint_id": "wc_endpoint_u7u7u7u7u7u7u7u7".to_string(),
        "expected_controller_generation": 1,
        "binding_id": binding_id,
    });
    let call = ToolCall::from_tool_name("agent_continuation_bind", args.clone()).unwrap();
    assert!(
        matches!(&call, ToolCall::AgentContinuationBind { binding_id: parsed, .. } if parsed == &binding_id)
    );
    let audit =
        crate::tool_audit::session_log_arguments_for_tool_request("agent_continuation_bind", &args)
            .to_string();
    assert!(!audit.contains("binding_id"));
    assert!(!audit.contains(&binding_id));
    args.as_object_mut().unwrap().remove("binding_id");
    assert!(ToolCall::from_tool_name("agent_continuation_bind", args).is_err());
}

#[test]
fn agent_wait_calls_parse_closed_selectors_and_keep_audit_payload_free() {
    const PRIVATE_TASK: &str = "wc_agent_task_ze-rze-rze-rze-r";
    const PRIVATE_KEY: &str = "PRIVATE_WAIT_KEY_MUST_NOT_PERSIST";
    let call = ToolCall::from_tool_name(
        "wait_for_agent_events",
        json!({
            "agent_id": "wc_dagent_iavN7wEjRWeJq83v",
            "endpoint_id": "wc_endpoint_iavN7wEjRWeJq83v",
            "expected_controller_generation": 4,
            "events": [{"kind":"agent_task_terminal","task_id":PRIVATE_TASK}],
            "idempotency_key": PRIVATE_KEY,
        }),
    )
    .unwrap();
    assert!(matches!(
        call,
        ToolCall::WaitForAgentEvents {
            expected_controller_generation: 4,
            ref events,
            ..
        } if events.len() == 1 && events[0].kind == "agent_task_terminal" && events[0].task_id == PRIVATE_TASK
    ));
    let audit = call.session_log_arguments();
    assert_eq!(audit["mode"], "any");
    assert_eq!(audit["event_count"], 1);
    assert_eq!(audit["idempotency_key_present"], true);
    let audit_text = audit.to_string();
    assert!(!audit_text.contains(PRIVATE_TASK));
    assert!(!audit_text.contains(PRIVATE_KEY));

    let read = ToolCall::from_tool_name(
        "read_agent_wait",
        json!({"wait_id": "wc_agent_wait_ZmZmZmZmZmZmZmZm".to_string()}),
    )
    .unwrap();
    assert!(matches!(read, ToolCall::ReadAgentWait { .. }));
    let state = ToolCall::from_tool_name(
        "agent_wait_state",
        json!({"wait_id": "wc_agent_wait_ZmZmZmZmZmZmZmZm".to_string()}),
    )
    .unwrap();
    assert!(matches!(state, ToolCall::AgentWaitState { .. }));
}

#[test]
fn work_on_project_parses_path_source_and_rejects_ambiguous_sources() {
    let work = ToolCall::from_tool_name(
        "work_on_project",
        json!({
            "client_id": "runner-1",
            "path": "/root/git/example",
            "instruction": "implement it"
        }),
    )
    .unwrap();
    assert!(work.project().is_none());
    let work_audit = work.session_log_arguments();
    assert_eq!(work_audit["path_source_requested"], true);
    assert!(work_audit.get("path").is_none());
    assert!(!work_audit.to_string().contains("/root/git/example"));

    for path in [
        r"C:\repo",
        "c:/repo",
        r"\\?\C:\repo",
        r"\\server\share\repo",
    ] {
        ToolCall::from_tool_name(
            "work_on_project",
            json!({"client_id": "runner-1", "path": path, "instruction": "implement it"}),
        )
        .unwrap_or_else(|error| panic!("work_on_project rejected {path:?}: {error}"));
    }

    for (tool, arguments) in [
        (
            "work_on_project",
            json!({"project": "agent:x:y", "path": "/tmp/y", "instruction": "x"}),
        ),
        (
            "work_on_project",
            json!({"client_id": "x", "instruction": "x"}),
        ),
        (
            "work_on_project",
            json!({"client_id": "x", "path": "relative/repo", "instruction": "x"}),
        ),
        (
            "work_on_project",
            json!({"client_id": "x", "path": r"\repo", "instruction": "x"}),
        ),
        (
            "work_on_project",
            json!({"client_id": "", "path": "/tmp/y", "instruction": "x"}),
        ),
    ] {
        let error = ToolCall::from_tool_name(tool, arguments).unwrap_err();
        assert!(
            error.contains("conflicting fields")
                || error.contains("missing ")
                || error.contains("must be an absolute")
                || error.contains("must not be empty"),
            "{error}"
        );
    }
}

#[test]
fn project_overview_tool_call_parses() {
    let call = ToolCall::from_tool_name(
        "project_overview",
        json!({
            "project": "agent:client:demo",
            "path": "crates/example",
            "max_depth": 3,
            "limit": 120
        }),
    )
    .unwrap();

    match call {
        ToolCall::ProjectOverview {
            project,
            path,
            max_depth,
            limit,
            ..
        } => {
            assert_eq!(project, "agent:client:demo");
            assert_eq!(path.as_deref(), Some("crates/example"));
            assert_eq!(max_depth, Some(3));
            assert_eq!(limit, Some(120));
        }
        other => panic!("expected ProjectOverview, got {other:?}"),
    }

    let audit_call = ToolCall::from_tool_name(
        "project_overview",
        json!({
            "project": "agent:client:demo",
            "path": "src",
            "max_depth": 2,
            "limit": 40
        }),
    )
    .unwrap();
    assert_eq!(
        audit_call.session_log_arguments(),
        json!({
            "project": "agent:client:demo",
            "path": "src",
            "max_depth": 2,
            "limit": 40
        })
    );
}

#[test]
fn observe_jobs_wake_policy_defaults_validates_and_audits_safely() {
    for (policy, expected) in [
        (None, ObserveJobsWakeOn::Change),
        (Some("change"), ObserveJobsWakeOn::Change),
        (Some("terminal"), ObserveJobsWakeOn::Terminal),
        (Some("all_terminal"), ObserveJobsWakeOn::AllTerminal),
    ] {
        let mut args = json!({
            "items": [{"job_id": "job", "after_observation_token": "private-observation-cursor"}],
            "wait_secs": 1
        });
        if let Some(policy) = policy {
            args["wake_on"] = json!(policy);
        }
        let call = ToolCall::from_tool_name("observe_jobs", args.clone()).unwrap();
        assert!(matches!(&call, ToolCall::ObserveJobs { wake_on, .. } if *wake_on == expected));
        let audit = call.session_log_arguments();
        assert_eq!(audit["wake_on"], serde_json::to_value(expected).unwrap());
        assert!(!audit.to_string().contains("private-observation-cursor"));
    }
    for policy in [
        json!("unknown-private-value"),
        json!(null),
        json!(1),
        json!({"bad": true}),
    ] {
        let args = json!({"items": [{"job_id": "job"}], "wake_on": policy});
        assert!(ToolCall::from_tool_name("observe_jobs", args.clone()).is_err());
    }
}

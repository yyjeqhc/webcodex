use super::super::kernel::{HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolTransport};
use super::super::sessions::{
    PostSessionMessageInput, SessionCreateOptions, SessionGuards, SessionMessageKind,
    SessionMessagePriority,
};
use super::super::{registered_tool_specs, ToolCall, ToolRuntime};
use super::support::*;
use crate::auth::scopes::{oauth_scope_policy_for_runtime_tool, OAuthToolScopePolicy};
use crate::auth::{SCOPE_RUNTIME_READ, SCOPE_SESSION_COLLABORATE};
use serde_json::{json, Value};

async fn call(
    runtime: &ToolRuntime,
    tool_name: &str,
    arguments: Value,
    auth: &crate::auth::AuthContext,
) -> super::super::ToolResult {
    let outcome = runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: tool_name.to_string(),
                arguments,
            },
            ToolCallContext {
                transport: ToolTransport::Api,
                session_id: None,
                auth: Some(auth),
                window: None,
                record_oauth_scope_denials: false,
                host_file_import_trust: HostFileImportTrust::Untrusted,
            },
        )
        .await;
    assert!(
        outcome.error_status.is_none(),
        "unexpected transport error: {:?}",
        outcome.error_status
    );
    outcome.result.expect("tool result")
}

fn authorized_session(runtime: &ToolRuntime, auth: &crate::auth::AuthContext) -> String {
    let owner = super::super::session_context::workflow_session_authority_fingerprint(Some(auth))
        .expect("stable test authority");
    runtime
        .sessions
        .start_session_with_options(
            SessionCreateOptions::new(
                None,
                Some("E3 assignment tool".to_string()),
                super::super::SessionMode::Normal,
                SessionGuards::default(),
            )
            .with_owner_authority_fingerprint(Some(owner)),
        )
        .unwrap()
        .session_id
}

fn post(
    runtime: &ToolRuntime,
    session_id: &str,
    kind: SessionMessageKind,
    body: &str,
    reply_to: Option<&str>,
) -> String {
    runtime
        .sessions
        .post_message(PostSessionMessageInput {
            session_id: session_id.to_string(),
            kind,
            message: body.to_string(),
            tags: Vec::new(),
            reply_to: reply_to.map(str::to_string),
            priority: SessionMessagePriority::Normal,
        })
        .unwrap()
        .message_id
}

#[tokio::test]
async fn e3_assignment_tool_round_trip_stale_projection_and_fresh_fence() {
    let runtime = test_runtime();
    let auth = auth_context(None, true);
    let session_id = authorized_session(&runtime, &auth);
    let todo_id = post(
        &runtime,
        &session_id,
        SessionMessageKind::Todo,
        "exact tool-level assignment",
        None,
    );
    let first_reply = post(
        &runtime,
        &session_id,
        SessionMessageKind::Guidance,
        "first direct guidance",
        Some(&todo_id),
    );

    let snapshot = call(
        &runtime,
        "get_session_assignment",
        json!({"session_id": session_id, "message_id": todo_id}),
        &auth,
    )
    .await;
    assert!(snapshot.success, "{:?}", snapshot.error);
    assert_eq!(snapshot.output["todo"]["message_id"], todo_id);
    assert_eq!(
        snapshot.output["direct_replies"][0]["message_id"],
        first_reply
    );
    let old_fence = snapshot.output["assignment_fence"]
        .as_str()
        .expect("assignment fence")
        .to_string();
    assert!(old_fence.starts_with("wsa1_"));

    post(
        &runtime,
        &session_id,
        SessionMessageKind::Progress,
        "unrelated body must not appear in stale projection",
        None,
    );
    let second_reply = post(
        &runtime,
        &session_id,
        SessionMessageKind::Note,
        "second direct reply",
        Some(&todo_id),
    );

    let stale = call(
        &runtime,
        "complete_session_message",
        json!({
            "session_id": session_id,
            "message_id": todo_id,
            "answer": "must not commit stale",
            "completion_key": "e3-tool-stale",
            "expected_assignment_fence": old_fence
        }),
        &auth,
    )
    .await;
    assert!(!stale.success);
    assert_eq!(stale.output["error_kind"], "assignment_stale");
    assert_eq!(stale.output["state_changed"], false);
    assert_eq!(
        stale.output["current_assignment"]["todo"]["message_id"],
        todo_id
    );
    assert_eq!(
        stale.output["current_assignment"]["direct_replies"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["message_id"],
        second_reply
    );
    let serialized = stale.output.to_string();
    assert!(!serialized.contains("unrelated body must not appear"));
    let fresh = stale.output["fresh_assignment_fence"]
        .as_str()
        .expect("fresh durable assignment fence")
        .to_string();
    assert!(fresh.starts_with("wsa1_"));
    assert_ne!(fresh, old_fence);

    let completed = call(
        &runtime,
        "complete_session_message",
        json!({
            "session_id": session_id,
            "message_id": todo_id,
            "answer": "fresh assignment accepted",
            "completion_key": "e3-tool-fresh",
            "expected_assignment_fence": fresh
        }),
        &auth,
    )
    .await;
    assert!(completed.success, "{:?}", completed.error);
    assert_eq!(completed.output["replayed"], false);
}

#[test]
fn e3_assignment_schema_parser_scope_local_coding_and_audit_are_synchronized() {
    let specs = registered_tool_specs();
    let get = specs
        .iter()
        .find(|spec| spec.name == "get_session_assignment")
        .expect("get_session_assignment public spec");
    assert_eq!(
        get.input_schema["required"],
        json!(["session_id", "message_id"])
    );
    assert_eq!(get.input_schema["additionalProperties"], false);
    assert_eq!(
        get.output_schema["properties"]["output"]["properties"]["direct_replies"]["maxItems"],
        16
    );
    assert_eq!(
        get.output_schema["properties"]["output"]["properties"]["assignment_fence"]["maxLength"],
        192
    );
    assert_eq!(
        get.output_schema["properties"]["output"]["properties"]["assignment_fence"]["pattern"],
        "^wsa1_[A-Za-z0-9_-]+$"
    );
    let complete = specs
        .iter()
        .find(|spec| spec.name == "complete_session_message")
        .expect("complete_session_message public spec");
    assert_eq!(
        complete.input_schema["properties"]["expected_assignment_fence"]["maxLength"],
        192
    );
    assert!(!complete.input_schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field == "expected_assignment_fence"));

    let get_call = ToolCall::from_tool_name(
        "get_session_assignment",
        json!({"session_id": "wc_sess_demo", "message_id": "wc_msg_demo"}),
    )
    .unwrap();
    assert!(matches!(
        get_call,
        ToolCall::GetSessionAssignment { ref session_id, ref message_id }
            if session_id == "wc_sess_demo" && message_id == "wc_msg_demo"
    ));
    let raw_fence = "wsa1_PRIVATE_FENCE_MUST_NOT_PERSIST";
    let completion_call = ToolCall::from_tool_name(
        "complete_session_message",
        json!({
            "session_id": "wc_sess_demo",
            "message_id": "wc_msg_demo",
            "answer": "done",
            "completion_key": "key",
            "expected_assignment_fence": raw_fence
        }),
    )
    .unwrap();
    assert!(matches!(
        completion_call,
        ToolCall::CompleteSessionMessage {
            expected_assignment_fence: Some(ref fence), ..
        } if fence == raw_fence
    ));

    assert!(
        crate::tool_runtime::tool_definition::LOCAL_CODING_TOOL_NAMES
            .contains(&"get_session_assignment")
    );
    assert!(
        crate::tool_runtime::tool_definition::LOCAL_CODING_TOOL_NAMES
            .contains(&"complete_session_message")
    );
    assert_eq!(
        oauth_scope_policy_for_runtime_tool("get_session_assignment"),
        OAuthToolScopePolicy::Require(SCOPE_RUNTIME_READ)
    );
    assert_eq!(
        oauth_scope_policy_for_runtime_tool("complete_session_message"),
        OAuthToolScopePolicy::Require(SCOPE_SESSION_COLLABORATE)
    );

    let input_audit = super::super::tool_audit::session_log_arguments_for_tool_request(
        "complete_session_message",
        &json!({
            "session_id": "wc_sess_demo",
            "message_id": "wc_msg_demo",
            "answer": "private answer",
            "completion_key": "private-key",
            "expected_assignment_fence": raw_fence
        }),
    );
    assert_eq!(input_audit["assignment_fence_present"], true);
    let input_text = input_audit.to_string();
    assert!(!input_text.contains(raw_fence));
    assert!(!input_text.contains("private answer"));
    assert!(!input_text.contains("private-key"));

    let output_audit = super::super::tool_audit::session_log_result_for_tool(
        "get_session_assignment",
        &json!({
            "success": true,
            "session_id": "wc_sess_demo",
            "message_id": "wc_msg_demo",
            "todo": {"message": "private todo"},
            "direct_replies": [{"message": "private reply"}],
            "assignment_fence": raw_fence
        }),
    );
    assert_eq!(output_audit["assignment_fence_present"], true);
    assert_eq!(output_audit["direct_reply_count"], 1);
    let output_text = output_audit.to_string();
    assert!(!output_text.contains(raw_fence));
    assert!(!output_text.contains("private todo"));
    assert!(!output_text.contains("private reply"));
}

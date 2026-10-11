//! Admission ordering and recorder evidence through the canonical entrypoint.
use crate::auth::{AuthContext, AuthKind};
use crate::tool_runtime::kernel::{
    HostFileImportTrust, ToolCallContext, ToolCallErrorStatus, ToolCallRequest, ToolTransport,
};
use crate::tool_runtime::sessions::{SessionCreateOptions, SessionGuards};
use crate::tool_runtime::{SessionMode, ToolRuntime};
use serde_json::json;

#[tokio::test]
async fn scope_denial_preserves_each_transports_recording_and_error_precedence() {
    for (transport, record_denial) in [(ToolTransport::Mcp, false), (ToolTransport::Api, true)] {
        let runtime = ToolRuntime::new_for_tests();
        let auth = AuthContext {
            user_id: Some("recorder-owner".into()),
            scopes: vec!["runtime:read".into()],
            ..AuthContext::new(AuthKind::OAuth2Token)
        };
        let fingerprint =
            crate::tool_runtime::workflow_session_authority_fingerprint(Some(&auth)).unwrap();
        let session = runtime
            .sessions
            .start_session_with_options(
                SessionCreateOptions::new(
                    None,
                    None,
                    SessionMode::Normal,
                    SessionGuards::default(),
                )
                .with_owner_authority_fingerprint(Some(fingerprint)),
            )
            .unwrap();
        let outcome = runtime
            .call_tool_with_context(
                ToolCallRequest {
                    tool_name: "read_files".into(),
                    // Deliberately invalid too: scope denial must win on both paths.
                    arguments: json!({}),
                },
                ToolCallContext {
                    transport,
                    session_id: Some(&session.session_id),
                    auth: Some(&auth),
                    window: None,
                    record_oauth_scope_denials: record_denial,
                    host_file_import_trust: HostFileImportTrust::Untrusted,
                },
            )
            .await;
        assert!(matches!(
            outcome.error_status,
            Some(ToolCallErrorStatus::InsufficientScope { .. })
        ));
        assert!(outcome.result.is_none());
        let summary = runtime
            .sessions
            .summary(&session.session_id, Some(10))
            .unwrap();
        assert_eq!(summary.counts.tool_calls, usize::from(record_denial));
        if record_denial {
            assert_eq!(summary.counts.failed, 1);
            assert_eq!(summary.events.len(), 2);
            assert_eq!(
                summary.events[1].error_kind.as_deref(),
                Some("insufficient_scope")
            );
        } else {
            assert!(summary.events.is_empty());
        }
    }
}

#[tokio::test]
async fn missing_app_capability_is_rejected_before_recorder_lookup_or_business_parsing() {
    let runtime = ToolRuntime::new_for_tests();
    let outcome = runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: "get_work_result_state".into(),
                arguments: json!({}),
            },
            ToolCallContext {
                transport: ToolTransport::Api,
                session_id: Some("~s999999"),
                auth: None,
                window: None,
                record_oauth_scope_denials: true,
                host_file_import_trust: HostFileImportTrust::Untrusted,
            },
        )
        .await;
    assert_eq!(outcome.error_status, Some(ToolCallErrorStatus::InvalidArguments {
        message: "Work Result App operations are available only on Stateless MCP 2026 requests with Work Result App capability".into(),
    }));
    assert!(outcome.result.is_none());
    assert!(outcome.canonical_audit_output.is_none());
}

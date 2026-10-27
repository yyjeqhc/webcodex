//! Session identity survives history/cache churn and restart; legacy tombstones
//! remain owner-scoped evidence, never a mechanism for new automatic expiry.
use super::super::kernel::{
    HostFileImportTrust, ToolCallContext, ToolCallOutcome, ToolCallRequest, ToolTransport,
};
use super::super::sessions::{SessionCreateOptions, SessionGuards, SessionLifecycle};
use super::super::{workflow_session_authority_fingerprint, SessionMode, ToolRuntime};
use crate::auth::{AuthContext, AuthKind};
use serde_json::json;
use std::sync::Arc;
use webcodex_core::model_reference::{format_model_reference, ModelReferenceKind};

fn user(name: &str) -> AuthContext {
    let mut auth = AuthContext::new(AuthKind::ApiToken);
    auth.user_id = Some(format!("user-{name}"));
    auth.username = Some(name.into());
    auth.api_key_id = Some(format!("key-{name}"));
    auth.token_kind = Some("user".into());
    auth.scopes = vec![crate::auth::SCOPE_RUNTIME_READ.into()];
    auth
}

fn runtime(directory: &std::path::Path) -> ToolRuntime {
    let path = directory.join("webcodex.db");
    ToolRuntime::new_for_tests()
        .with_project_reference_database(Arc::new(crate::Database::open(&path).unwrap()))
        .with_session_database(&path, &directory.join("sessions.json"))
        .unwrap()
}

fn start(runtime: &ToolRuntime, auth: &AuthContext) -> String {
    runtime
        .sessions
        .start_session_with_options(
            SessionCreateOptions::new(
                None,
                Some("persistent identity".into()),
                SessionMode::Normal,
                SessionGuards::default(),
            )
            .with_owner_authority_fingerprint(Some(
                workflow_session_authority_fingerprint(Some(auth)).unwrap(),
            )),
        )
        .unwrap()
        .session_id
}

async fn summary(runtime: &ToolRuntime, selector: &str, auth: &AuthContext) -> ToolCallOutcome {
    runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: "read_session_summary".into(),
                arguments: json!({"session_id":selector}),
            },
            ToolCallContext {
                transport: ToolTransport::Api,
                session_id: None,
                auth: Some(auth),
                window: None,
                record_oauth_scope_denials: true,
                host_file_import_trust: HostFileImportTrust::Untrusted,
            },
        )
        .await
}

#[tokio::test]
async fn session_refs_survive_closed_history_churn_cold_reads_and_restart() {
    let directory = tempfile::tempdir().unwrap();
    let first_runtime = runtime(directory.path());
    let alice = user("alice");
    let active = start(&first_runtime, &alice);
    let closed = start(&first_runtime, &alice);
    let active_ref = first_runtime
        .session_reference_for_id(&active, Some(&alice))
        .unwrap();
    let closed_ref = first_runtime
        .session_reference_for_id(&closed, Some(&alice))
        .unwrap();
    let created_at = first_runtime.sessions.session_created_at(&active).unwrap();
    first_runtime.sessions.close_session(&closed).unwrap();
    for _ in 0..250 {
        let newer = start(&first_runtime, &alice);
        first_runtime.sessions.close_session(&newer).unwrap();
    }
    first_runtime.sessions.flush().await.unwrap();
    drop(first_runtime);
    let restored = runtime(directory.path());
    assert_eq!(restored.sessions.status().hot_sessions, 0);
    for (id, reference, lifecycle) in [
        (&active, &active_ref, "active"),
        (&closed, &closed_ref, "closed"),
    ] {
        let read = summary(&restored, reference, &alice).await;
        assert!(read.success, "{read:?}");
        let output = read.result.unwrap().output;
        assert_eq!(output["session_id"], *id);
        assert_eq!(output["lifecycle"], lifecycle);
        assert_eq!(
            restored.session_reference_for_id(id, Some(&alice)).as_ref(),
            Some(reference)
        );
    }
    assert_eq!(
        restored.sessions.session_created_at(&active),
        Some(created_at)
    );
    assert_eq!(restored.sessions.status().retained_sessions, 252);
    assert_eq!(
        restored.sessions.status().hot_sessions,
        0,
        "reads must not pin historical records"
    );
    assert_eq!(
        restored.sessions.lifecycle_state(&closed),
        Some(SessionLifecycle::Closed)
    );
}

#[tokio::test]
async fn retained_refs_keep_principal_and_incarnation_authority() {
    let directory = tempfile::tempdir().unwrap();
    let runtime = runtime(directory.path());
    let alice = user("alice");
    let bob = user("bob");
    let id = start(&runtime, &alice);
    let reference = runtime.session_reference_for_id(&id, Some(&alice)).unwrap();
    runtime.sessions.flush().await.unwrap();
    assert!(!summary(&runtime, &reference, &bob).await.success);
    let foreign = summary(&runtime, &id, &bob).await;
    assert!(!foreign.success);
    assert_eq!(
        foreign.result.unwrap().output["error_kind"],
        "session_authority_denied"
    );
    let principal = workflow_session_authority_fingerprint(Some(&alice)).unwrap();
    let db = crate::Database::open(&directory.path().join("webcodex.db")).unwrap();
    let stale = db
        .get_or_create_model_reference(
            &principal,
            ModelReferenceKind::Session,
            &id,
            &"c".repeat(64),
            1,
        )
        .unwrap();
    let stale_ref = format_model_reference(ModelReferenceKind::Session, stale.ref_index);
    assert!(!summary(&runtime, &stale_ref, &alice).await.success);
    assert!(summary(&runtime, &reference, &alice).await.success);
    assert_eq!(runtime.sessions.status().retained_sessions, 1);
}

#[tokio::test]
async fn imported_expiry_is_distinct_for_the_owner_and_never_retargets() {
    let directory = tempfile::tempdir().unwrap();
    let alice = user("alice");
    let bob = user("bob");
    let owner = workflow_session_authority_fingerprint(Some(&alice)).unwrap();
    let id = "wc_sess_0123456789abcdef";
    let incarnation =
        webcodex_workflow_session::workflow_session_incarnation_fingerprint(id, 1, None, &owner);
    std::fs::write(directory.path().join("sessions.json"), serde_json::to_vec(&json!({
        "version":2, "sessions":[{"retention_tombstone":{
            "session_id":id,"owner_authority_fingerprint":owner,"incarnation_fingerprint":incarnation,"expiry_ordinal":1
        }}]
    })).unwrap()).unwrap();
    let runtime = runtime(directory.path());
    let db = crate::Database::open(&directory.path().join("webcodex.db")).unwrap();
    let saved = db
        .get_or_create_model_reference(&owner, ModelReferenceKind::Session, id, &incarnation, 1)
        .unwrap();
    let reference = format_model_reference(ModelReferenceKind::Session, saved.ref_index);
    for selector in [id, &reference] {
        let owned = summary(&runtime, selector, &alice).await;
        assert!(!owned.success);
        assert_eq!(
            owned.result.unwrap().output["error_kind"],
            "session_retention_expired"
        );
        let foreign = summary(&runtime, selector, &bob).await;
        assert!(!foreign.success);
        if let Some(result) = foreign.result {
            assert_eq!(result.output["error_kind"], "unknown_session_id");
            assert!(!result.output.to_string().contains("incarnation"));
        }
    }
    assert_eq!(runtime.sessions.status().retained_sessions, 0);
    assert!(runtime
        .sessions
        .allocate_fixed_session_suffix_for_test("0123456789abcdef")
        .is_none());
}

#[tokio::test]
async fn unavailable_payload_keeps_exact_refs_and_reports_storage_instead_of_expiry() {
    let directory = tempfile::tempdir().unwrap();
    let first = runtime(directory.path());
    let alice = user("alice");
    let id = start(&first, &alice);
    first.sessions.flush().await.unwrap();
    let reference = first.session_reference_for_id(&id, Some(&alice)).unwrap();
    drop(first);
    let restored = runtime(directory.path());
    // Keep a valid identity index and make only its backing payload unreadable.
    let connection = rusqlite::Connection::open(directory.path().join("webcodex.db")).unwrap();
    connection
        .execute(
            "UPDATE workflow_sessions SET state_json='invalid JSON' WHERE session_id=?1",
            [&id],
        )
        .unwrap();
    for selector in [&id, &reference] {
        let read = summary(&restored, selector, &alice).await;
        assert!(!read.success);
        let result = read.result.unwrap();
        assert_eq!(result.output["error_kind"], "session_storage_unavailable");
        assert_eq!(result.output["state_changed"], false);
    }
    assert_eq!(
        restored
            .session_reference_for_id(&id, Some(&alice))
            .as_ref(),
        Some(&reference)
    );
    assert_eq!(restored.sessions.status().retained_sessions, 1);
    assert_eq!(restored.sessions.status().retention_tombstones, 0);
}

#[tokio::test]
async fn failed_creation_fence_keeps_one_identity_and_blocks_more_admission_until_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let runtime = runtime(directory.path());
    let alice = user("alice");
    runtime.sessions.set_persistence_failure_for_test(true);
    async fn create(runtime: &ToolRuntime, auth: &AuthContext) -> ToolCallOutcome {
        runtime
            .call_tool_with_context(
                ToolCallRequest {
                    tool_name: "start_session".into(),
                    arguments: json!({"title":"durability fixture"}),
                },
                ToolCallContext {
                    transport: ToolTransport::Api,
                    session_id: None,
                    auth: Some(auth),
                    window: None,
                    record_oauth_scope_denials: true,
                    host_file_import_trust: HostFileImportTrust::Untrusted,
                },
            )
            .await
    }
    let first = create(&runtime, &alice).await;
    assert!(!first.success);
    let output = first.result.unwrap().output;
    assert_eq!(output["error_kind"], "session_persistence_uncertain");
    assert_eq!(output["state_changed"], true);
    let id = output["session_id"].as_str().unwrap().to_owned();
    let second = create(&runtime, &alice).await;
    assert!(!second.success);
    let output = second.result.unwrap().output;
    assert_eq!(output["error_kind"], "session_storage_unavailable");
    assert_eq!(output["state_changed"], false);
    assert_eq!(runtime.sessions.status().retained_sessions, 1);
    runtime.sessions.set_persistence_failure_for_test(false);
    runtime.sessions.flush().await.unwrap();
    drop(runtime);
    let restored = self::runtime(directory.path());
    assert!(summary(&restored, &id, &alice).await.success);
    assert_eq!(restored.sessions.status().retained_sessions, 1);
}

//! Owner-scoped retention tombstones at the runtime Session boundary.
//!
//! A matching principal learns that this exact Closed Session aged out of
//! historical retention. Every other caller keeps the existing unknown result.

use std::sync::Arc;

use serde_json::{json, Value};
use webcodex_core::model_reference::{format_model_reference, ModelReferenceKind};

use super::super::kernel::{
    HostFileImportTrust, ToolCallContext, ToolCallErrorStatus, ToolCallOutcome, ToolCallRequest,
    ToolTransport,
};
use super::super::session_context::workflow_session_incarnation_fingerprint;
use super::super::sessions::{
    SessionCreateOptions, SessionGuards, SessionLifecycle, SessionStore, SessionTransport,
};
use super::super::tool_inputs::CodingGuidanceProfile;
use super::super::window_activity::ToolCallCorrelation;
use super::super::{
    workflow_session_authority_fingerprint, SessionMode, ToolCall, ToolResult, ToolRuntime,
};
use super::reconnect::dispatch_coding_call_in_window;
use super::support::{auth_context, init_git_repo, register_runner_project_at_path};
use crate::auth::{AuthContext, AuthKind};
use webcodex_tool_contracts::SessionLifecycleInput;

struct Fixture {
    _tmp: tempfile::TempDir,
    runtime: ToolRuntime,
}

fn fixture(historical_limit: usize) -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let mut runtime = ToolRuntime::new_for_tests().with_project_reference_database(Arc::new(
        crate::Database::open(&tmp.path().join("session-refs.db")).unwrap(),
    ));
    runtime.sessions = SessionStore::new_in_memory_with_limits(8, historical_limit, 64);
    Fixture { _tmp: tmp, runtime }
}

fn api_user(name: &str) -> AuthContext {
    let mut auth = AuthContext::new(AuthKind::ApiToken);
    auth.user_id = Some(format!("user-{name}"));
    auth.username = Some(name.to_string());
    auth.api_key_id = Some(format!("key-{name}"));
    auth.token_kind = Some("user".to_string());
    // session_summary admits only callers that hold runtime:read. The scope is
    // not part of the Workflow Session authority fingerprint.
    auth.scopes = vec![crate::auth::SCOPE_RUNTIME_READ.to_string()];
    auth
}

fn start(runtime: &ToolRuntime, auth: Option<&AuthContext>, title: &str) -> String {
    let owner = workflow_session_authority_fingerprint(auth).unwrap();
    runtime
        .sessions
        .start_session_with_options(
            SessionCreateOptions::new(
                None,
                Some(title.to_string()),
                SessionMode::Normal,
                SessionGuards::default(),
            )
            .with_owner_authority_fingerprint(Some(owner)),
        )
        .unwrap()
        .session_id
}

fn close(runtime: &ToolRuntime, session_id: &str) {
    runtime.sessions.close_session(session_id).unwrap();
}

/// Historical limit 1: the first Closed Session becomes the tombstone and the
/// second remains the retained Closed row.
fn tombstone_oldest(runtime: &ToolRuntime, auth: Option<&AuthContext>) -> (String, String) {
    let expired = start(runtime, auth, "expired");
    close(runtime, &expired);
    let retained = start(runtime, auth, "retained");
    close(runtime, &retained);
    assert!(runtime
        .sessions
        .retention_tombstone_for_test(&expired)
        .is_some());
    assert!(!runtime.sessions.contains_session(&expired));
    assert_eq!(
        runtime.sessions.lifecycle_state(&retained),
        Some(SessionLifecycle::Closed)
    );
    (expired, retained)
}

fn context<'a>(
    auth: Option<&'a AuthContext>,
    recording_session_id: Option<&'a str>,
) -> ToolCallContext<'a> {
    ToolCallContext {
        transport: ToolTransport::Api,
        session_id: recording_session_id,
        auth,
        window: None,
        record_oauth_scope_denials: true,
        host_file_import_trust: HostFileImportTrust::Untrusted,
    }
}

async fn call_summary(
    runtime: &ToolRuntime,
    session_id: &str,
    auth: Option<&AuthContext>,
    recording_session_id: Option<&str>,
) -> ToolCallOutcome {
    runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: "session_summary".to_string(),
                arguments: json!({ "session_id": session_id }),
            },
            context(auth, recording_session_id),
        )
        .await
}

fn result_of(outcome: &ToolCallOutcome) -> &ToolResult {
    assert!(!outcome.success, "{outcome:?}");
    assert!(outcome.error_status.is_none(), "{outcome:?}");
    outcome
        .result
        .as_ref()
        .expect("canonical session lookup returns a structured result")
}

fn assert_no_tombstone_metadata(value: &Value) {
    let encoded = value.to_string();
    for forbidden in [
        "owner_authority_fingerprint",
        "incarnation_fingerprint",
        "expiry_ordinal",
        "created_at",
        "retention_tombstone",
        "tombstone",
    ] {
        assert!(
            !encoded.contains(forbidden),
            "{forbidden} leaked into {encoded}"
        );
    }
}

fn assert_retention_expired(outcome: &ToolCallOutcome, session_id: &str) {
    let result = result_of(outcome);
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "session_retention_expired");
    assert_eq!(result.output["session_id"], session_id);
    assert_eq!(result.output["state_changed"], false);
    assert_eq!(result.output["recovery_kind"], "none");
    assert_eq!(
        result.error.as_deref(),
        Some(
            format!(
                "session_retention_expired: {session_id} existed, but its Closed-session historical retention expired"
            )
            .as_str()
        )
    );
    assert_no_tombstone_metadata(&result.output);
    assert!(!result
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("fingerprint"));
}

fn assert_unknown_session(outcome: &ToolCallOutcome, session_id: &str) {
    let result = result_of(outcome);
    assert_eq!(result.output["error_kind"], "unknown_session_id");
    assert_eq!(result.output["session_id"], session_id);
    assert_eq!(result.output["recovery_kind"], "fix_input");
    assert!(result.output.get("state_changed").is_none());
    assert_eq!(
        result.error.as_deref(),
        Some(format!("unknown_session_id: {session_id}").as_str())
    );
    assert_no_tombstone_metadata(&result.output);
    let encoded = format!(
        "{} {}",
        result.error.as_deref().unwrap_or_default(),
        result.output
    );
    assert!(!encoded.contains("retention"));
    assert!(!encoded.contains("expired"));
}

fn unknown_shape(result: &ToolResult) -> (String, Value) {
    let session_id = result.output["session_id"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let error = result
        .error
        .clone()
        .unwrap_or_default()
        .replace(&session_id, "<session>");
    let mut output = result.output.clone();
    if let Some(object) = output.as_object_mut() {
        object.insert("session_id".to_string(), json!("<session>"));
    }
    (error, output)
}

fn assert_unknown_ref(outcome: &ToolCallOutcome, raw: &str) {
    assert!(!outcome.success, "{outcome:?}");
    assert!(outcome.result.is_none(), "{outcome:?}");
    match &outcome.error_status {
        Some(ToolCallErrorStatus::InvalidArguments { message }) => {
            assert!(message.contains("unknown_session_ref"), "{message}");
            assert!(message.contains(raw), "{message}");
            assert!(!message.contains("retention"), "{message}");
            assert!(!message.contains("expired"), "{message}");
        }
        other => panic!("expected unknown_session_ref, got {other:?}"),
    }
}

#[tokio::test]
async fn owner_canonical_id_is_retention_expired_and_records_that_kind() {
    let fixture = fixture(1);
    let runtime = &fixture.runtime;
    let recorder = start(runtime, None, "recorder");
    let (expired, retained) = tombstone_oldest(runtime, None);
    let before = runtime.sessions.status().retained_sessions;

    let direct = call_summary(runtime, &expired, None, None).await;
    assert_retention_expired(&direct, &expired);
    assert!(runtime.sessions.summary(&expired, Some(20)).is_none());
    assert_eq!(runtime.sessions.status().retained_sessions, before);

    let recorded = call_summary(runtime, &expired, None, Some(&recorder)).await;
    assert_retention_expired(&recorded, &expired);
    let summary = runtime.sessions.summary(&recorder, Some(20)).unwrap();
    let finished = summary
        .events
        .iter()
        .find(|event| event.kind == "tool_call_finished" && event.tool_name == "session_summary")
        .expect("live recorder keeps the failed lookup");
    assert_eq!(
        finished.error_kind.as_deref(),
        Some("session_retention_expired")
    );
    assert_eq!(
        finished.actual_failure_kind.as_deref(),
        Some("session_retention_expired")
    );
    assert_eq!(
        runtime.sessions.lifecycle_state(&retained),
        Some(SessionLifecycle::Closed)
    );
    let retained_summary = call_summary(runtime, &retained, None, None).await;
    assert!(retained_summary.success, "{:?}", retained_summary.result);
    assert_eq!(
        retained_summary.result.unwrap().output["lifecycle"],
        "closed"
    );
}

#[tokio::test]
async fn foreign_canonical_id_matches_a_never_issued_id() {
    let fixture = fixture(1);
    let runtime = &fixture.runtime;
    let alice = api_user("alice");
    let bob = api_user("bob");
    let (expired, _) = tombstone_oldest(runtime, Some(&alice));
    let never_issued = "wc_sess_0123456789abcdef0123456789abcdef";

    let foreign = call_summary(runtime, &expired, Some(&bob), None).await;
    let absent = call_summary(runtime, never_issued, Some(&bob), None).await;
    assert_unknown_session(&foreign, &expired);
    assert_unknown_session(&absent, never_issued);
    assert_eq!(
        unknown_shape(result_of(&foreign)),
        unknown_shape(result_of(&absent))
    );
    assert!(runtime
        .sessions
        .retention_tombstone_for_test(&expired)
        .is_some());
}

#[tokio::test]
async fn live_foreign_session_stays_authority_denied() {
    let fixture = fixture(2);
    let runtime = &fixture.runtime;
    let alice = api_user("alice");
    let bob = api_user("bob");
    let session_id = start(runtime, Some(&alice), "alice live");

    let outcome = call_summary(runtime, &session_id, Some(&bob), None).await;
    let result = result_of(&outcome);
    assert_eq!(result.output["error_kind"], "session_authority_denied");
    assert_eq!(result.output["session_id"], session_id);
    assert_eq!(result.output["state_changed"], false);
    assert_eq!(result.output["recovery_kind"], "user_action");
    assert!(runtime.sessions.contains_session(&session_id));
}

#[tokio::test]
async fn owner_session_ref_expires_only_when_the_incarnation_matches() {
    let fixture = fixture(1);
    let runtime = &fixture.runtime;
    let expired = start(runtime, None, "pinned");
    let session_ref = runtime
        .session_reference_for_id(&expired, None)
        .expect("live session issues a ref");
    let principal = workflow_session_authority_fingerprint(None).unwrap();
    let stale = runtime
        .project_reference_db
        .as_ref()
        .unwrap()
        .get_or_create_model_reference(
            &principal,
            ModelReferenceKind::Session,
            &expired,
            &"c".repeat(64),
            1,
        )
        .unwrap();
    let stale_ref = format_model_reference(ModelReferenceKind::Session, stale.ref_index);
    assert_ne!(stale_ref, session_ref);
    close(runtime, &expired);
    let _retained = start(runtime, None, "newer");
    close(runtime, &_retained);
    assert!(runtime
        .sessions
        .retention_tombstone_for_test(&expired)
        .is_some());

    let matched = call_summary(runtime, &session_ref, None, None).await;
    assert_retention_expired(&matched, &expired);
    let mut stale_arguments = json!({ "session_id": stale_ref.clone() });
    let error = runtime
        .canonicalize_session_reference_argument(&mut stale_arguments, None)
        .unwrap_err();
    assert!(error.to_string().contains("unknown_session_ref"));
    assert_eq!(stale_arguments["session_id"], stale_ref);
    assert_unknown_ref(
        &call_summary(runtime, &stale_ref, None, None).await,
        &stale_ref,
    );
}

#[tokio::test]
async fn foreign_malformed_and_missing_refs_stay_unknown() {
    let fixture = fixture(1);
    let runtime = &fixture.runtime;
    let alice = api_user("alice");
    let bob = api_user("bob");
    let expired = start(runtime, Some(&alice), "alice expired");
    let session_ref = runtime
        .session_reference_for_id(&expired, Some(&alice))
        .unwrap();
    close(runtime, &expired);
    let retained = start(runtime, Some(&alice), "alice retained");
    close(runtime, &retained);

    let mut foreign = json!({ "session_id": session_ref.clone() });
    let error = runtime
        .canonicalize_session_reference_argument(&mut foreign, Some(&bob))
        .unwrap_err();
    assert!(error.to_string().contains("unknown_session_ref"));
    assert_eq!(foreign["session_id"], session_ref);
    assert_unknown_ref(
        &call_summary(runtime, &session_ref, Some(&bob), None).await,
        &session_ref,
    );

    for raw in ["~s01", "~s999"] {
        let mut arguments = json!({ "session_id": raw });
        let error = runtime
            .canonicalize_session_reference_argument(&mut arguments, Some(&alice))
            .unwrap_err();
        assert!(error.to_string().contains("unknown_session_ref"), "{raw}");
        assert_eq!(arguments["session_id"], raw);
        assert_unknown_ref(&call_summary(runtime, raw, Some(&alice), None).await, raw);
    }
}

#[tokio::test]
async fn pruned_tombstone_becomes_unknown_and_does_not_retarget_the_ref() {
    let fixture = fixture(1);
    let runtime = &fixture.runtime;
    let first = start(runtime, None, "first");
    let first_ref = runtime.session_reference_for_id(&first, None).unwrap();
    close(runtime, &first);
    let second = start(runtime, None, "second");
    let second_ref = runtime.session_reference_for_id(&second, None).unwrap();
    close(runtime, &second);
    assert_retention_expired(&call_summary(runtime, &first, None, None).await, &first);
    assert_retention_expired(&call_summary(runtime, &first_ref, None, None).await, &first);

    let third = start(runtime, None, "third");
    let third_ref = runtime.session_reference_for_id(&third, None).unwrap();
    close(runtime, &third);
    assert!(runtime
        .sessions
        .retention_tombstone_for_test(&first)
        .is_none());
    assert!(runtime
        .sessions
        .retention_tombstone_for_test(&second)
        .is_some());
    assert_eq!(
        runtime.sessions.lifecycle_state(&third),
        Some(SessionLifecycle::Closed)
    );

    assert_unknown_session(&call_summary(runtime, &first, None, None).await, &first);
    let mut stale = json!({ "session_id": first_ref.clone() });
    let error = runtime
        .canonicalize_session_reference_argument(&mut stale, None)
        .unwrap_err();
    assert!(error.to_string().contains("unknown_session_ref"));
    assert_eq!(stale["session_id"], first_ref);
    assert_unknown_ref(
        &call_summary(runtime, &first_ref, None, None).await,
        &first_ref,
    );
    assert_ne!(first_ref, third_ref);
    assert_eq!(
        runtime
            .canonicalize_explicit_session_selector(&third_ref, None)
            .unwrap(),
        third
    );
    assert_retention_expired(&call_summary(runtime, &second, None, None).await, &second);
    assert_retention_expired(
        &call_summary(runtime, &second_ref, None, None).await,
        &second,
    );
}

#[tokio::test]
async fn explicit_resume_of_an_expired_id_does_not_create_a_session() {
    let fixture = fixture(1);
    let runtime = &fixture.runtime;
    let (expired, _) = tombstone_oldest(runtime, None);
    let before = runtime.sessions.status();
    let mut correlation = ToolCallCorrelation::default();
    let mut bootstrap = None;
    let result = runtime
        .work_on_project(
            "agent:test:retention".to_string(),
            None,
            None,
            Some("checkout".to_string()),
            None,
            "resume the expired session".to_string(),
            Some(expired.clone()),
            CodingGuidanceProfile::Direct,
            false,
            None,
            None,
            None,
            SessionTransport::Api,
            &mut correlation,
            &mut bootstrap,
        )
        .await;
    assert!(!result.success, "{:?}", result.error);
    assert_eq!(result.output["error_kind"], "session_retention_expired");
    assert_eq!(result.output["session_id"], expired);
    assert_eq!(result.output["state_changed"], false);
    assert_eq!(result.output["recovery_kind"], "none");
    assert_no_tombstone_metadata(&result.output);
    let after = runtime.sessions.status();
    assert_eq!(after.retained_sessions, before.retained_sessions);
    assert_eq!(after.active_sessions, before.active_sessions);
    assert!(!runtime.sessions.contains_session(&expired));

    let mut invalid_correlation = ToolCallCorrelation::default();
    let invalid = runtime
        .work_on_project(
            "agent:test:retention".to_string(),
            None,
            None,
            Some("checkout".to_string()),
            None,
            "resume by ref".to_string(),
            Some("~s1".to_string()),
            CodingGuidanceProfile::Direct,
            false,
            None,
            None,
            None,
            SessionTransport::Api,
            &mut invalid_correlation,
            &mut bootstrap,
        )
        .await;
    assert_eq!(invalid.output["error_kind"], "invalid_session_id");
    assert_eq!(
        runtime.sessions.status().retained_sessions,
        before.retained_sessions
    );
}

#[tokio::test]
async fn owner_expired_session_ref_on_the_work_on_project_kernel_does_not_create_a_session() {
    let fixture = fixture(1);
    let runtime = &fixture.runtime;
    let expired = start(runtime, None, "expired ref");
    let session_ref = runtime
        .session_reference_for_id(&expired, None)
        .expect("live session issues a ref before retention");
    assert!(session_ref.starts_with("~s"), "{session_ref}");
    close(runtime, &expired);
    let retained = start(runtime, None, "retained");
    close(runtime, &retained);
    assert!(runtime
        .sessions
        .retention_tombstone_for_test(&expired)
        .is_some());
    let before = runtime.sessions.status();

    let outcome = runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: "work_on_project".to_string(),
                arguments: json!({
                    "project": "agent:test:retention",
                    "instruction": "resume the expired session by ref",
                    "session_id": session_ref,
                }),
            },
            context(None, None),
        )
        .await;

    assert_retention_expired(&outcome, &expired);
    let after = runtime.sessions.status();
    assert_eq!(after.active_sessions, before.active_sessions);
    assert_eq!(after.retained_sessions, before.retained_sessions);
    assert_eq!(after.closed_sessions, before.closed_sessions);
    assert_eq!(after.retention_tombstones, before.retention_tombstones);
    assert_eq!(after.capacity_evictions, before.capacity_evictions);
    assert!(!runtime.sessions.contains_session(&expired));
    assert_eq!(
        runtime.sessions.lifecycle_state(&retained),
        Some(SessionLifecycle::Closed)
    );
}

#[tokio::test]
async fn omitted_session_id_still_creates_a_fresh_session() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let mut runtime = ToolRuntime::new_for_tests();
    runtime.sessions = SessionStore::new_in_memory_with_limits(8, 1, 64);
    let auth = auth_context(None, true);
    let expired = start(&runtime, Some(&auth), "expired");
    close(&runtime, &expired);
    let retained = start(&runtime, Some(&auth), "retained");
    close(&runtime, &retained);
    assert!(runtime
        .sessions
        .retention_tombstone_for_test(&expired)
        .is_some());
    let before = runtime.sessions.status().retained_sessions;
    register_runner_project_at_path(&runtime, "retention-create", "demo", root.path()).await;

    let result = dispatch_coding_call_in_window(
        &runtime,
        "retention-create",
        ToolCall::WorkOnProject {
            project: "demo".to_string(),
            client_id: None,
            path: None,
            mode: None,
            base_ref: None,
            instruction: "start a fresh session beside a tombstone".to_string(),
            guidance_profile: Some(CodingGuidanceProfile::Direct),
            include_extension_catalog: false,
            session_id: None,
        },
        Some(&auth),
        "retention-create-window",
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    let created = result.output["session_id"].as_str().unwrap();
    assert!(created.starts_with("wc_sess_"));
    assert_ne!(created, expired);
    assert_ne!(created, retained);
    assert_eq!(result.output["continuation"], "created");
    assert_eq!(runtime.sessions.status().retained_sessions, before + 1);
    assert_eq!(
        runtime.sessions.lifecycle_state(created),
        Some(SessionLifecycle::Active)
    );
    assert!(runtime
        .sessions
        .retention_tombstone_for_test(&expired)
        .is_some());
}

#[tokio::test]
async fn retained_closed_ref_resolves_to_that_closed_session() {
    let fixture = fixture(2);
    let runtime = &fixture.runtime;
    let session_id = start(runtime, None, "still retained");
    let session_ref = runtime.session_reference_for_id(&session_id, None).unwrap();
    close(runtime, &session_id);
    assert!(runtime
        .sessions
        .retention_tombstone_for_test(&session_id)
        .is_none());

    assert_eq!(
        runtime
            .canonicalize_explicit_session_selector(&session_ref, None)
            .unwrap(),
        session_id
    );
    let outcome = call_summary(runtime, &session_ref, None, None).await;
    assert!(outcome.success, "{:?}", outcome.result);
    let result = outcome.result.unwrap();
    assert_eq!(result.output["session_id"], session_id);
    assert_eq!(result.output["lifecycle"], "closed");
    assert_eq!(
        runtime.sessions.lifecycle_state(&session_id),
        Some(SessionLifecycle::Closed)
    );
}

#[test]
fn runtime_incarnation_matches_the_prune_time_fingerprint() {
    let fixture = fixture(1);
    let runtime = &fixture.runtime;
    let owner = workflow_session_authority_fingerprint(None).unwrap();
    let session_id = start(runtime, None, "fingerprint");
    let summary = runtime.sessions.summary(&session_id, Some(1)).unwrap();
    let runtime_fingerprint = workflow_session_incarnation_fingerprint(
        &session_id,
        summary.created_at,
        summary.project.as_deref(),
        &owner,
    );
    let crate_fingerprint = webcodex_workflow_session::workflow_session_incarnation_fingerprint(
        &session_id,
        summary.created_at,
        summary.project.as_deref(),
        &owner,
    );
    assert_eq!(runtime_fingerprint, crate_fingerprint);
    assert_eq!(
        webcodex_workflow_session::workflow_session_incarnation_fingerprint(
            &session_id,
            summary.created_at,
            None,
            &owner,
        ),
        webcodex_workflow_session::workflow_session_incarnation_fingerprint(
            &session_id,
            summary.created_at,
            Some(""),
            &owner,
        )
    );
    close(runtime, &session_id);
    let retained = start(runtime, None, "kept");
    close(runtime, &retained);
    let tombstone = runtime
        .sessions
        .retention_tombstone_for_test(&session_id)
        .unwrap();
    assert_eq!(tombstone.incarnation_fingerprint, runtime_fingerprint);
    assert_eq!(tombstone.owner_authority_fingerprint, owner);
}

#[tokio::test]
async fn restored_ledger_keeps_the_owner_lookup_distinct_from_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sessions.json");
    let owner = workflow_session_authority_fingerprint(None).unwrap();
    let expired = {
        let store = SessionStore::with_persistence_limits(&path, 8, 1, 32);
        let expired = store
            .start_session_with_options(
                SessionCreateOptions::new(
                    None,
                    Some("expired".to_string()),
                    SessionMode::Normal,
                    SessionGuards::default(),
                )
                .with_owner_authority_fingerprint(Some(owner.clone())),
            )
            .unwrap()
            .session_id;
        store.close_session(&expired).unwrap();
        let retained = store
            .start_session_with_options(
                SessionCreateOptions::new(
                    None,
                    Some("retained".to_string()),
                    SessionMode::Normal,
                    SessionGuards::default(),
                )
                .with_owner_authority_fingerprint(Some(owner)),
            )
            .unwrap()
            .session_id;
        store.close_session(&retained).unwrap();
        store.flush_persistence();
        assert!(store.retention_tombstone_for_test(&expired).is_some());
        expired
    };

    let runtime = ToolRuntime::new_for_tests().with_session_ledger(&path);
    assert_eq!(runtime.sessions.status().retention_tombstones, 1);
    assert_retention_expired(
        &call_summary(&runtime, &expired, None, None).await,
        &expired,
    );
    let bob = api_user("bob");
    assert_unknown_session(
        &call_summary(&runtime, &expired, Some(&bob), None).await,
        &expired,
    );
    let never_issued = "wc_sess_fedcba9876543210fedcba9876543210";
    assert_unknown_session(
        &call_summary(&runtime, never_issued, None, None).await,
        never_issued,
    );
}

#[tokio::test]
async fn owner_expired_canonical_id_on_the_work_on_project_kernel_does_not_create_a_session() {
    let fixture = fixture(1);
    let runtime = &fixture.runtime;
    let project =
        register_runner_project_at_path(runtime, "retention-kernel", "demo", fixture._tmp.path())
            .await;
    let auth = auth_context(None, true);
    let owner = workflow_session_authority_fingerprint(Some(&auth)).unwrap();
    let expired = runtime
        .sessions
        .start_session_with_options(
            SessionCreateOptions::new(
                Some(project.clone()),
                Some("expired canonical".to_string()),
                SessionMode::Normal,
                SessionGuards::default(),
            )
            .with_owner_authority_fingerprint(Some(owner.clone())),
        )
        .unwrap()
        .session_id;
    close(runtime, &expired);
    let retained = runtime
        .sessions
        .start_session_with_options(
            SessionCreateOptions::new(
                Some(project.clone()),
                Some("retained canonical".to_string()),
                SessionMode::Normal,
                SessionGuards::default(),
            )
            .with_owner_authority_fingerprint(Some(owner)),
        )
        .unwrap()
        .session_id;
    close(runtime, &retained);
    assert!(runtime
        .sessions
        .retention_tombstone_for_test(&expired)
        .is_some());
    let before = runtime.sessions.status();

    let outcome = runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: "work_on_project".to_string(),
                arguments: json!({
                    "project": project,
                    "instruction": "resume the expired session by canonical id",
                    "session_id": expired,
                }),
            },
            context(Some(&auth), None),
        )
        .await;

    assert_retention_expired(&outcome, &expired);
    let after = runtime.sessions.status();
    assert_eq!(after.active_sessions, before.active_sessions);
    assert_eq!(after.retained_sessions, before.retained_sessions);
    assert_eq!(after.closed_sessions, before.closed_sessions);
    assert_eq!(after.retention_tombstones, before.retention_tombstones);
    assert!(!runtime.sessions.contains_session(&expired));
    assert_eq!(
        runtime.sessions.lifecycle_state(&retained),
        Some(SessionLifecycle::Closed)
    );
}

#[tokio::test]
async fn list_sessions_omits_tombstones_while_exact_lookup_stays_expired() {
    let root = tempfile::tempdir().unwrap();
    let mut runtime = ToolRuntime::new_for_tests().with_project_reference_database(Arc::new(
        crate::Database::open(&root.path().join("refs.db")).unwrap(),
    ));
    runtime.sessions = SessionStore::new_in_memory_with_limits(8, 1, 64);
    let project =
        register_runner_project_at_path(&runtime, "retention-list", "demo", root.path()).await;
    let auth = auth_context(None, true);
    let foreign = api_user("foreign-list");

    let expired = runtime
        .sessions
        .start_session_with_options(
            SessionCreateOptions::new(
                Some(project.clone()),
                Some("expired list".to_string()),
                SessionMode::Normal,
                SessionGuards::default(),
            )
            .with_owner_authority_fingerprint(Some(
                workflow_session_authority_fingerprint(Some(&auth)).unwrap(),
            )),
        )
        .unwrap()
        .session_id;
    let expired_ref = runtime
        .session_reference_for_id(&expired, Some(&auth))
        .expect("live session issues a ref");
    close(&runtime, &expired);
    let closed = runtime
        .sessions
        .start_session_with_options(
            SessionCreateOptions::new(
                Some(project.clone()),
                Some("retained closed".to_string()),
                SessionMode::Normal,
                SessionGuards::default(),
            )
            .with_owner_authority_fingerprint(Some(
                workflow_session_authority_fingerprint(Some(&auth)).unwrap(),
            )),
        )
        .unwrap()
        .session_id;
    close(&runtime, &closed);
    let active = runtime
        .sessions
        .start_session_with_options(
            SessionCreateOptions::new(
                Some(project.clone()),
                Some("retained active".to_string()),
                SessionMode::Normal,
                SessionGuards::default(),
            )
            .with_owner_authority_fingerprint(Some(
                workflow_session_authority_fingerprint(Some(&auth)).unwrap(),
            )),
        )
        .unwrap()
        .session_id;
    assert!(runtime
        .sessions
        .retention_tombstone_for_test(&expired)
        .is_some());

    let listed = runtime
        .dispatch_with_auth(
            ToolCall::ListSessions {
                project: project.clone(),
                lifecycle: None,
                offset: None,
                limit: None,
            },
            Some(&auth),
        )
        .await;
    assert!(listed.success, "{:?}", listed.error);
    assert_eq!(listed.output["total"], 2);
    let ids = listed.output["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["session_id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert!(ids.contains(&closed));
    assert!(ids.contains(&active));
    assert!(!ids.contains(&expired));
    let encoded = listed.output.to_string();
    assert!(!encoded.contains(&expired));
    assert!(!encoded.contains(&expired_ref));
    assert!(encoded.contains("session_ref"));

    for (lifecycle, expected) in [
        (SessionLifecycleInput::Closed, &closed),
        (SessionLifecycleInput::Active, &active),
    ] {
        let page = runtime
            .dispatch_with_auth(
                ToolCall::ListSessions {
                    project: project.clone(),
                    lifecycle: Some(lifecycle),
                    offset: None,
                    limit: None,
                },
                Some(&auth),
            )
            .await;
        assert!(page.success, "{:?}", page.error);
        assert_eq!(page.output["total"], 1);
        assert_eq!(page.output["sessions"][0]["session_id"], *expected);
        assert!(!page.output.to_string().contains(&expired));
    }

    assert_retention_expired(
        &call_summary(&runtime, &expired, Some(&auth), None).await,
        &expired,
    );
    assert_retention_expired(
        &call_summary(&runtime, &expired_ref, Some(&auth), None).await,
        &expired,
    );
    let foreign_lookup = call_summary(&runtime, &expired, Some(&foreign), None).await;
    let foreign_missing = call_summary(
        &runtime,
        "wc_sess_0123456789abcdef0123456789abcdef",
        Some(&foreign),
        None,
    )
    .await;
    assert_eq!(
        unknown_shape(result_of(&foreign_lookup)),
        unknown_shape(result_of(&foreign_missing))
    );
}

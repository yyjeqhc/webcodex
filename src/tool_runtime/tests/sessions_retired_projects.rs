//! Owner, inventory, identity, and durability fences for retired Project history.
use super::super::kernel::{
    HostFileImportTrust, ToolCallContext, ToolCallOutcome, ToolCallRequest, ToolTransport,
};
use super::super::*;
use super::support::*;
use crate::auth::AuthContext;
use crate::runner_protocol::{RunnerCapabilities, RunnerRegisterRequest};
use serde_json::json;

const CLIENT: &str = "retired-project";
const PROJECT: &str = "agent:retired-project:repo";

fn owner(admin: bool) -> AuthContext {
    let mut auth = auth_context(Some("alice"), false);
    auth.scopes = if admin {
        vec!["admin".into()]
    } else {
        vec![
            "runtime:read".into(),
            "session:collaborate".into(),
            "project:read".into(),
            "project:write".into(),
            "job:run".into(),
        ]
    };
    auth
}

async fn register(runtime: &ToolRuntime, username: &str, root: Option<&str>) {
    register_agent_projects(
        runtime,
        CLIENT,
        Some(username),
        RunnerCapabilities::default(),
        root.map(|root| registered_project("repo", root))
            .into_iter()
            .collect(),
    )
    .await;
}

async fn call(
    runtime: &ToolRuntime,
    auth: Option<&AuthContext>,
    tool: &str,
    arguments: serde_json::Value,
    recorder: Option<&str>,
) -> ToolCallOutcome {
    runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: tool.into(),
                arguments,
            },
            ToolCallContext {
                transport: ToolTransport::Api,
                session_id: recorder,
                auth,
                window: None,
                record_oauth_scope_denials: true,
                host_file_import_trust: HostFileImportTrust::Untrusted,
            },
        )
        .await
}

async fn start(runtime: &ToolRuntime, auth: &AuthContext) -> String {
    let result = call(
        runtime,
        Some(auth),
        "start_session",
        json!({"project":PROJECT}),
        None,
    )
    .await;
    assert!(result.success, "{result:?}");
    result.result.unwrap().output["session_id"]
        .as_str()
        .unwrap()
        .into()
}

async fn unregister(runtime: &ToolRuntime) {
    assert!(runtime
        .runner_registry
        .remove_runner_project_for_instance(CLIENT, &format!("inst-{CLIENT}"), "repo",)
        .await
        .unwrap());
}

#[tokio::test]
async fn runtime_unregister_receipt_preserves_authorized_historical_close() {
    let runtime = test_runtime();
    let auth = owner(false);
    let revision = format!("sha256:{}", "a".repeat(64));
    let mut project = registered_project("repo", "/original-root");
    project.revision = Some(revision.clone());
    register_agent_projects(
        &runtime,
        CLIENT,
        Some("alice"),
        RunnerCapabilities {
            project_lifecycle: true,
            ..Default::default()
        },
        vec![project],
    )
    .await;
    let id = start(&runtime, &auth).await;

    // Drive the real Server unregister service through a Runner request/receipt,
    // rather than deleting the Registry entry directly as other unit tests do.
    let request_runtime = runtime.clone();
    let request_auth = auth.clone();
    let unregister_task = tokio::spawn(async move {
        crate::admin_project_lifecycle::unregister_project_runtime(
            &request_runtime,
            Some(&request_auth),
            PROJECT,
            &revision,
        )
        .await
    });
    let request =
        wait_for_runner_request_for_instance(&runtime, CLIENT, &format!("inst-{CLIENT}")).await;
    assert_eq!(request.kind, "project_lifecycle_unregister");
    let webcodex_core::runner_operation::RunnerOperation::Project(operation) =
        request.decode_operation().unwrap()
    else {
        panic!("expected a typed Project lifecycle operation");
    };
    let request_payload: serde_json::Value = serde_json::from_str(&operation.payload).unwrap();
    assert_eq!(request_payload["project_id"], "repo");
    complete_patch_agent_request_for_instance(
        &runtime,
        CLIENT,
        &format!("inst-{CLIENT}"),
        &request.request_id,
        0,
        &json!({"outcome":"unregistered","changed":true,"revision":null}).to_string(),
        "",
    )
    .await;
    let reply = unregister_task.await.unwrap();
    assert_eq!(reply.status, 200, "{reply:?}");
    assert_eq!(reply.body["outcome"], "unregistered");

    let history = call(
        &runtime,
        Some(&auth),
        "read_session_summary",
        json!({"session_id":id}),
        None,
    )
    .await;
    assert!(
        history.success,
        "historical read should survive unregister: {history:?}"
    );

    let execution = call(
        &runtime,
        Some(&auth),
        "update_session_context",
        json!({"project":PROJECT,"session_id":id,"execution_context":{"default_cwd":"src"}}),
        None,
    )
    .await;
    assert!(
        !execution.success,
        "retired Project must not admit live mutation"
    );

    let close = call(
        &runtime,
        Some(&auth),
        "close_session",
        json!({"session_id":id}),
        None,
    )
    .await;
    assert!(close.success, "authorized historical close: {close:?}");
    assert_eq!(close.result.unwrap().output["already_closed"], false);
    assert_eq!(
        runtime.sessions.lifecycle_state(&id),
        Some(sessions::SessionLifecycle::Closed)
    );
}

#[tokio::test]
async fn retired_project_history_requires_current_runner_owner_even_for_admin() {
    for admin in [true, false] {
        let runtime = test_runtime();
        let auth = owner(admin);
        register(&runtime, "alice", Some("/original-root")).await;
        let id = start(&runtime, &auth).await;
        unregister(&runtime).await;
        register(&runtime, "bob", None).await;
        for tool in ["read_session_summary", "close_session"] {
            let result = call(&runtime, Some(&auth), tool, json!({"session_id":id}), None).await;
            assert!(
                !result.success,
                "admin visibility must not bypass revoked Runner ownership: {result:?}"
            );
        }
        assert_eq!(
            runtime.sessions.lifecycle_state(&id),
            Some(sessions::SessionLifecycle::Active)
        );
    }
}

#[tokio::test]
async fn retired_project_history_supports_scoped_owner_and_kernel_recorder() {
    for admin in [true, false] {
        let runtime = test_runtime();
        let auth = owner(admin);
        register(&runtime, "alice", Some("/original-root")).await;
        let id = start(&runtime, &auth).await;
        unregister(&runtime).await;
        for recorder in [None, Some(id.as_str())] {
            let result = call(
                &runtime,
                Some(&auth),
                "read_session_summary",
                json!({"session_id":id}),
                recorder,
            )
            .await;
            assert!(
                result.success,
                "owner history (admin={admin}, recorder={recorder:?}): {result:?}"
            );
        }
        for already_closed in [false, true] {
            let result = call(
                &runtime,
                Some(&auth),
                "close_session",
                json!({"session_id":id}),
                Some(&id),
            )
            .await;
            assert!(result.success, "{result:?}");
            assert_eq!(
                result.result.unwrap().output["already_closed"],
                already_closed
            );
        }
        let result = call(
            &runtime,
            Some(&auth),
            "read_session_summary",
            json!({"session_id":id}),
            Some(&id),
        )
        .await;
        assert!(result.success, "{result:?}");
        assert_eq!(result.result.unwrap().output["lifecycle"], "closed");
        for denied_auth in [
            Some(auth_context(Some("bob"), true)),
            Some(open_auth_context()),
            None,
        ] {
            let result = call(
                &runtime,
                denied_auth.as_ref(),
                "read_session_summary",
                json!({"session_id":id}),
                None,
            )
            .await;
            assert!(!result.success, "foreign/anonymous history: {result:?}");
        }
        let mut revoked_scope = auth.clone();
        revoked_scope.scopes.clear();
        assert!(
            !call(
                &runtime,
                Some(&revoked_scope),
                "close_session",
                json!({"session_id":id}),
                None
            )
            .await
            .success
        );
    }
}

#[tokio::test]
async fn retired_project_pending_inventory_does_not_prove_unregistration() {
    let runtime = test_runtime();
    let auth = owner(true);
    register(&runtime, "alice", Some("/original-root")).await;
    let id = start(&runtime, &auth).await;
    unregister(&runtime).await;
    runtime
        .runner_registry
        .register(RunnerRegisterRequest {
            owner: Some("alice".into()),
            ..crate::test_support::runner_registration(
                CLIENT,
                &format!("inst-{CLIENT}"),
                RunnerCapabilities::default(),
            )
        })
        .await
        .unwrap();
    let view = runtime
        .runner_registry
        .get_runner_view(CLIENT)
        .await
        .unwrap();
    assert_ne!(view.project_inventory.unwrap().sync_state, "complete");
    for tool in ["read_session_summary", "close_session"] {
        let result = call(&runtime, Some(&auth), tool, json!({"session_id":id}), None).await;
        assert!(
            !result.success,
            "partial inventory is not removal evidence: {result:?}"
        );
    }
    register(&runtime, "alice", None).await;
    assert!(
        call(
            &runtime,
            Some(&auth),
            "close_session",
            json!({"session_id":id}),
            None
        )
        .await
        .success
    );
}

#[tokio::test]
async fn retired_project_reused_id_cannot_resume_old_session() {
    let runtime = test_runtime();
    let auth = owner(false);
    register(&runtime, "alice", Some("/original-root")).await;
    let id = start(&runtime, &auth).await;
    unregister(&runtime).await;
    register(&runtime, "alice", Some("/replacement-root")).await;
    for tool in ["read_session_summary", "close_session"] {
        let response = call(&runtime, Some(&auth), tool, json!({"session_id":id}), None).await;
        assert!(
            !response.success,
            "reused root should reject history access"
        );
    }
    let result = call(
        &runtime,
        Some(&auth),
        "update_session_context",
        json!({
            "project":PROJECT, "session_id":id, "execution_context":{"default_cwd":"src"}
        }),
        None,
    )
    .await;
    assert!(
        !result.success,
        "same textual Project must not restore Session execution authority: {result:?}"
    );
    assert_eq!(
        result.result.unwrap().output["error_kind"],
        "session_project_identity_changed"
    );
    let result = call(&runtime, Some(&auth), "list_projects", json!({}), Some(&id)).await;
    assert!(
        !result.success,
        "a replaced Project cannot be a live recorder: {result:?}"
    );
}

#[tokio::test]
async fn retired_project_same_root_reregistration_cannot_resume_old_session() {
    let runtime = test_runtime();
    let auth = owner(false);
    register(&runtime, "alice", Some("/original-root")).await;
    let id = start(&runtime, &auth).await;
    unregister(&runtime).await;
    register(&runtime, "alice", Some("/original-root")).await;

    let result = call(
        &runtime,
        Some(&auth),
        "update_session_context",
        json!({
            "project": PROJECT,
            "session_id": id,
            "execution_context": {"default_cwd": "src"}
        }),
        None,
    )
    .await;
    assert!(
        !result.success,
        "re-registering the same root must not revive a retired Session: {result:?}"
    );

    // A stale Session must not enqueue a Runner command on the replacement,
    // even when the Project selector and canonical root are byte-for-byte equal.
    let forbidden = runtime
        .dispatch_with_auth(
            ToolCall::RunShell {
                login: false,
                project: PROJECT.to_string(),
                command: "echo stale".to_string(),
                session_id: Some(id),
                timeout_secs: Some(10),
                sync_wait_secs: None,
                cwd: None,
                purpose: None,
                shell: None,
            },
            Some(&auth),
        )
        .await;
    assert!(
        !forbidden.success,
        "stale Session must not execute: {forbidden:?}"
    );
    assert_eq!(
        forbidden.output["error_kind"],
        "session_project_identity_changed"
    );
    assert_ne!(forbidden.output["command_started"], true);
    assert!(
        probe_agent_request_for_instance(&runtime, CLIENT, &format!("inst-{CLIENT}"))
            .await
            .is_none()
    );
}

#[tokio::test]
async fn project_inventory_reconnect_keeps_registration_identity() {
    let runtime = test_runtime();
    let auth = owner(false);
    let project = registered_project("repo", "/original-root");
    let original_incarnation = project.registration_incarnation.clone();
    assert!(original_incarnation.is_some());

    register_agent_projects(
        &runtime,
        CLIENT,
        Some("alice"),
        RunnerCapabilities::default(),
        vec![project.clone()],
    )
    .await;
    let id = start(&runtime, &auth).await;

    // Reconnect/re-publish the same persisted registration, not a fresh one.
    register_agent_projects(
        &runtime,
        CLIENT,
        Some("alice"),
        RunnerCapabilities::default(),
        vec![project],
    )
    .await;

    let result = call(
        &runtime,
        Some(&auth),
        "update_session_context",
        json!({"project": PROJECT, "session_id": id,
            "execution_context": {"default_cwd": "src"}}),
        None,
    )
    .await;
    assert!(
        result.success,
        "a reconnect must not retire the same registration: {result:?}"
    );
}

#[tokio::test]
async fn local_dev_session_keeps_live_access_and_retired_history_without_auth_context() {
    let runtime = test_runtime();
    register(&runtime, "alice", Some("/original-root")).await;
    let created = call(
        &runtime,
        None,
        "start_session",
        json!({"project": PROJECT}),
        None,
    )
    .await;
    assert!(created.success, "{created:?}");
    let session_id = created.result.unwrap().output["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        runtime
            .authorize_session_target(&session_id, "update_session_context", None)
            .await
            .is_ok(),
        "the original registration must remain usable"
    );
    unregister(&runtime).await;

    // Historical access is permitted only after complete, authoritative
    // removal; it must not restore the retired Session's execution privileges.
    let history = call(
        &runtime,
        None,
        "read_session_summary",
        json!({"session_id": session_id}),
        None,
    )
    .await;
    assert!(
        history.success,
        "historical read should remain available to local/dev: {history:?}"
    );
    assert!(
        runtime
            .authorize_session_target(&session_id, "update_session_context", None)
            .await
            .is_err(),
        "retired Project cannot be used for live mutation"
    );
    let closed = call(
        &runtime,
        None,
        "close_session",
        json!({"session_id": session_id}),
        None,
    )
    .await;
    assert!(
        closed.success,
        "authorized local/dev historical close: {closed:?}"
    );
}

#[tokio::test]
async fn local_dev_session_rejects_reused_project_registration_without_auth_context() {
    let runtime = test_runtime();
    register(&runtime, "alice", Some("/original-root")).await;
    let created = call(
        &runtime,
        None,
        "start_session",
        json!({"project": PROJECT}),
        None,
    )
    .await;
    assert!(
        created.success,
        "local-dev must be able to start a Project Session: {created:?}"
    );
    let session_id = created.result.unwrap().output["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    unregister(&runtime).await;
    // Same Project ID AND same canonical root, but a new persisted registration.
    register(&runtime, "alice", Some("/original-root")).await;

    // Local/dev's canonical authority must be subject to the *same*
    // registration fence as authenticated callers, before any tool policy.
    let denied = runtime
        .authorize_session_target(&session_id, "update_session_context", None)
        .await
        .err()
        .expect("the replaced registration must reject a local-dev Session");
    assert_eq!(
        denied.output["error_kind"],
        "session_project_identity_changed"
    );

    let updated = call(
        &runtime,
        None,
        "update_session_context",
        json!({
            "project": PROJECT,
            "session_id": session_id,
            "execution_context": {"default_cwd": "src"}
        }),
        None,
    )
    .await;
    assert!(
        !updated.success,
        "local-dev stale Session must not mutate a replacement Project: {updated:?}"
    );

    let forbidden = runtime
        .dispatch_with_auth(
            ToolCall::RunShell {
                login: false,
                project: PROJECT.into(),
                command: "echo stale".into(),
                session_id: Some(session_id),
                timeout_secs: Some(10),
                sync_wait_secs: None,
                cwd: None,
                purpose: None,
                shell: None,
            },
            None,
        )
        .await;
    assert!(
        !forbidden.success,
        "local-dev stale Session must not dispatch to replacement Runner: {forbidden:?}"
    );
    assert_eq!(
        forbidden.output["error_kind"],
        "session_project_identity_changed"
    );
    assert_ne!(forbidden.output["command_started"], true);
    assert!(
        probe_agent_request_for_instance(&runtime, CLIENT, &format!("inst-{CLIENT}"))
            .await
            .is_none()
    );
}

#[tokio::test]
async fn legacy_registration_without_incarnation_cannot_create_live_session() {
    let runtime = test_runtime();
    let auth = owner(false);
    let mut legacy = registered_project("repo", "/original-root");
    legacy.registration_incarnation = None;
    register_agent_projects(
        &runtime,
        CLIENT,
        Some("alice"),
        RunnerCapabilities::default(),
        vec![legacy],
    )
    .await;
    let result = call(
        &runtime,
        Some(&auth),
        "start_session",
        json!({"project": PROJECT}),
        None,
    )
    .await;
    assert!(
        !result.success,
        "legacy path-only identity is not execution authority: {result:?}"
    );
    assert_eq!(
        result.result.unwrap().output["error_kind"],
        "project_registration_identity_unavailable"
    );
}

#[tokio::test]
async fn retired_project_close_commits_before_success_and_survives_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("webcodex.db");
    let legacy = directory.path().join("sessions.json");
    let runtime = test_runtime()
        .with_session_database(&database, &legacy)
        .unwrap();
    let auth = owner(false);
    register(&runtime, "alice", Some("/original-root")).await;
    let id = start(&runtime, &auth).await;
    unregister(&runtime).await;
    let result = call(
        &runtime,
        Some(&auth),
        "close_session",
        json!({"session_id":id}),
        None,
    )
    .await;
    assert!(result.success, "{result:?}");
    // Reopen while the original writer is alive; dropping it would mask a missing flush.
    let restored = test_runtime()
        .with_session_database(&database, &legacy)
        .unwrap();
    assert_eq!(
        restored.sessions.session_project_identity(&id),
        runtime.sessions.session_project_identity(&id),
        "the durable Project root identity must survive a SQLite reopen"
    );
    register(&restored, "alice", None).await;
    assert_eq!(
        restored.sessions.lifecycle_state(&id),
        Some(sessions::SessionLifecycle::Closed)
    );
    let result = call(
        &restored,
        Some(&auth),
        "close_session",
        json!({"session_id":id}),
        Some(&id),
    )
    .await;
    assert!(result.success, "{result:?}");
    assert_eq!(result.result.unwrap().output["already_closed"], true);
}

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

use super::support::*;
use crate::runner_protocol::{RunnerCapabilities, RunnerProjectSummary};
use crate::tool_runtime::projects::ListProjectsOptions;
use crate::tool_runtime::{ToolCall, ToolRuntime};
use serde_json::json;
use std::sync::Arc;

fn root(hex: char) -> String {
    format!("wc_projroot_{}", hex.to_string().repeat(64))
}

fn project(
    client_id: &str,
    id: &str,
    name: &str,
    path: &str,
    fingerprint: char,
) -> RunnerProjectSummary {
    let mut project = named_registered_project(client_id, id, name, path, 1);
    project.root_fingerprint = Some(root(fingerprint));
    project
}

fn project_ref_for(listed: &crate::tool_runtime::ToolResult, canonical: &str) -> String {
    listed.output["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|project| project["id"] == canonical)
        .and_then(|project| project["project_ref"].as_str())
        .unwrap()
        .to_string()
}

fn runtime_with_reference_db(path: &std::path::Path) -> ToolRuntime {
    let db = Arc::new(crate::Database::open(&path.to_path_buf()).unwrap());
    ToolRuntime::new_for_tests().with_project_reference_database(db)
}

async fn dispatch_shell_success(
    runtime: &ToolRuntime,
    client_id: &str,
    auth: &crate::auth::AuthContext,
    project_selector: &str,
    expected_cwd: &str,
) {
    let call = ToolCall::from_tool_name(
        "run_shell",
        json!({
            "project": project_selector,
            "command": "pwd",
            "timeout_secs": 10,
            "sync_wait_secs": 10
        }),
    )
    .unwrap();
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        async move { runtime.dispatch_with_auth(call, Some(&auth)).await }
    });
    let request = wait_for_patch_agent_request(runtime, client_id).await;
    assert_eq!(request.command, "pwd");
    assert_eq!(request.cwd.as_deref(), Some(expected_cwd));
    complete_patch_agent_request(runtime, client_id, &request.request_id, 0, expected_cwd, "")
        .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
}

#[tokio::test]
async fn work_result_accepts_only_exact_issued_project_refs_not_friendly_aliases() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_reference_db(&tmp.path().join("refs.db"));
    register_agent_projects(
        &runtime,
        "special",
        None,
        RunnerCapabilities::default(),
        vec![project("special", "demo", "friendly", "/srv/demo", '1')],
    )
    .await;
    let auth = auth_context(None, true);
    let resolved = runtime
        .resolve_project_input_for_auth("agent:special:demo", Some(&auth))
        .await
        .unwrap();
    let reference = runtime
        .project_reference_for_resolved(&resolved, Some(&auth))
        .unwrap();
    assert!(reference.starts_with("~p"));
    // The exact Work Result resolver is the common path for card creation and
    // refresh. Later resource URLs continue using the resolved canonical ID.
    let accepted = runtime
        .authorize_work_result_project(&reference, Some(&auth))
        .await
        .unwrap();
    assert_eq!(accepted, "agent:special:demo");
    assert!(runtime
        .authorize_work_result_project("friendly", Some(&auth))
        .await
        .is_err());
    assert!(runtime
        .authorize_work_result_project("~p99999", Some(&auth))
        .await
        .is_err());
    assert!(runtime
        .authorize_work_result_project(&reference, Some(&auth_context(Some("other"), false)))
        .await
        .is_err());
    register_agent_projects(
        &runtime,
        "special",
        None,
        RunnerCapabilities::default(),
        vec![project("special", "demo", "friendly", "/srv/replaced", '2')],
    )
    .await;
    assert!(runtime
        .authorize_work_result_project(&reference, Some(&auth))
        .await
        .is_err());
}

#[tokio::test]
async fn project_refs_route_exact_projects_without_bare_name_uniqueness() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_reference_db(&tmp.path().join("refs.db"));
    register_agent_projects(
        &runtime,
        "special",
        None,
        RunnerCapabilities::default(),
        vec![
            project(
                "special",
                "webcodex",
                "WebCodex special",
                "/srv/special",
                '1',
            ),
            project("special", "alpha-id", "friendly", "/srv/alpha", '2'),
        ],
    )
    .await;
    register_agent_projects(
        &runtime,
        "oe",
        None,
        RunnerCapabilities::default(),
        vec![project("oe", "webcodex", "WebCodex oe", "/srv/oe", '3')],
    )
    .await;

    let listed = runtime
        .list_projects_with_options(None, ListProjectsOptions::default())
        .await;
    assert!(listed.success, "{:?}", listed.error);
    let special = "agent:special:webcodex";
    let oe = "agent:oe:webcodex";
    let special_ref = project_ref_for(&listed, special);
    let oe_ref = project_ref_for(&listed, oe);
    assert_ne!(special_ref, oe_ref);
    assert!(special_ref.starts_with("~p"));
    assert!(oe_ref.starts_with("~p"));

    assert_eq!(
        runtime
            .resolve_project_input(&special_ref)
            .await
            .unwrap()
            .resolved_id,
        special
    );
    assert_eq!(
        runtime
            .resolve_project_input(&oe_ref)
            .await
            .unwrap()
            .resolved_id,
        oe
    );
    assert!(
        runtime.resolve_project_input("webcodex").await.is_err(),
        "duplicate bare project ids must remain ambiguous"
    );
    assert_eq!(
        runtime
            .resolve_project_input("special:webcodex")
            .await
            .unwrap()
            .resolved_id,
        special
    );
    assert_eq!(
        runtime
            .resolve_project_input(special)
            .await
            .unwrap()
            .resolved_id,
        special
    );
    assert_eq!(
        runtime
            .resolve_project_input("alpha-id")
            .await
            .unwrap()
            .resolved_id,
        "agent:special:alpha-id"
    );
    assert_eq!(
        runtime
            .resolve_project_input("friendly")
            .await
            .unwrap()
            .resolved_id,
        "agent:special:alpha-id"
    );

    let relisted = runtime
        .list_projects_with_options(None, ListProjectsOptions::default())
        .await;
    assert_eq!(project_ref_for(&relisted, special), special_ref);
    assert_eq!(project_ref_for(&relisted, oe), oe_ref);
}

#[tokio::test]
async fn project_refs_are_principal_scoped_and_reauthorize_visibility() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_reference_db(&tmp.path().join("refs.db"));
    register_agent_projects(
        &runtime,
        "private",
        Some("alice"),
        RunnerCapabilities::default(),
        vec![project("private", "repo", "repo", "/srv/private", '4')],
    )
    .await;
    let canonical = "agent:private:repo";
    let alice = auth_context(Some("alice"), false);
    let resolved = runtime
        .resolve_project_input_for_auth(canonical, Some(&alice))
        .await
        .unwrap();
    let project_ref = runtime
        .project_reference_for_resolved(&resolved, Some(&alice))
        .unwrap();

    let bob = auth_context(Some("bob"), false);
    assert!(
        runtime
            .resolve_project_input_for_auth(&project_ref, Some(&bob))
            .await
            .is_err(),
        "another principal must not learn or use Alice's short mapping"
    );

    let mut same_principal_revoked = alice.clone();
    same_principal_revoked.username = Some("bob".to_string());
    assert!(
        runtime
            .resolve_project_input_for_auth(&project_ref, Some(&same_principal_revoked))
            .await
            .is_err(),
        "short ref must rerun current Runner visibility even for the same principal namespace"
    );
}

#[tokio::test]
async fn server_issued_project_ref_crosses_real_project_scoped_dispatch_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_reference_db(&tmp.path().join("dispatch-refs.db"));
    let client_id = "project-ref-dispatch";
    let canonical = "agent:project-ref-dispatch:repo";
    let original_path = "/srv/project-ref-dispatch";
    let replacement_path = "/srv/project-ref-dispatch-replacement";
    let alice = auth_context(Some("alice"), false);
    register_agent_with_projects(
        &runtime,
        client_id,
        Some("alice"),
        RunnerCapabilities {
            shell: true,
            file_read: true,
            git: true,
            internal_posix_script: true,
            ..Default::default()
        },
        vec![project(client_id, "repo", "repo", original_path, '7')],
    )
    .await;

    let listed = runtime
        .dispatch_with_auth(
            ToolCall::ListProjects {
                include_git_summary: false,
                client_id: None,
                project: None,
                query: None,
                limit: None,
                summary_only: false,
            },
            Some(&alice),
        )
        .await;
    assert!(listed.success, "{:?}", listed.error);
    let project_ref = project_ref_for(&listed, canonical);
    assert!(project_ref.starts_with("~p"));

    // The exact Server-issued selector must cross the real model-facing shell
    // dispatch path and reach the Project that issued it.
    dispatch_shell_success(&runtime, client_id, &alice, &project_ref, original_path).await;

    // A representative read-only Project tool already consumes ResolvedProject;
    // keep it in this regression so both dispatch styles stay aligned.
    let read = ToolCall::from_tool_name(
        "read_files",
        json!({"project": project_ref, "items": [{"path": "README.md", "limit": 2}]}),
    )
    .unwrap();
    let read_task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = alice.clone();
        async move { runtime.dispatch_with_auth(read, Some(&auth)).await }
    });
    let read_request = wait_for_patch_agent_request(&runtime, client_id).await;
    assert_eq!(read_request.kind, "file_read");
    assert_eq!(read_request.path.as_deref(), Some("README.md"));
    complete_agent_ranged_file_read_request(&runtime, client_id, &read_request, "one\ntwo\n").await;
    let read_result = read_task.await.unwrap();
    assert!(read_result.success, "{:?}", read_result.error);
    assert_eq!(read_result.output["project"], canonical);

    // Git uses a specialized dispatcher that historically repeated the same
    // auth-less resolution mistake as shell. Exercise that boundary too.
    let git = ToolCall::from_tool_name("get_git_status", json!({"project": project_ref})).unwrap();
    let git_task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = alice.clone();
        async move { runtime.dispatch_with_auth(git, Some(&auth)).await }
    });
    let git_request = wait_for_patch_agent_request(&runtime, client_id).await;
    assert_eq!(git_request.command, "git status --porcelain");
    assert_eq!(git_request.cwd.as_deref(), Some(original_path));
    complete_patch_agent_request(&runtime, client_id, &git_request.request_id, 0, "", "").await;
    let git_result = git_task.await.unwrap();
    assert!(git_result.success, "{:?}", git_result.error);

    // Full canonical ids remain first-class selectors.
    dispatch_shell_success(&runtime, client_id, &alice, canonical, original_path).await;

    // A different principal cannot use or infer Alice's mapping.
    let bob = auth_context(Some("bob"), false);
    let bob_result = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name(
                "run_shell",
                json!({"project": project_ref, "command": "pwd"}),
            )
            .unwrap(),
            Some(&bob),
        )
        .await;
    assert!(!bob_result.success);
    let bob_projection = format!(
        "{} {}",
        bob_result.error.as_deref().unwrap_or_default(),
        bob_result.output
    );
    assert!(bob_projection.contains("unknown_project"));
    assert!(!bob_projection.contains(canonical));
    assert!(!bob_projection.contains(original_path));
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());

    // Keep Alice's stable principal identity while changing the current Runner
    // visibility identity. The old ref must be reauthorized and fail closed.
    let mut alice_without_visibility = alice.clone();
    alice_without_visibility.username = Some("bob".to_string());
    let revoked_result = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name(
                "run_shell",
                json!({"project": project_ref, "command": "pwd"}),
            )
            .unwrap(),
            Some(&alice_without_visibility),
        )
        .await;
    assert!(!revoked_result.success);
    assert!(revoked_result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("unknown_project")));
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());

    // Re-register the same canonical id at a different root identity. The old
    // short ref is fenced to the original fingerprint and must never retarget.
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        client_id,
        "inst",
        vec![project(client_id, "repo", "repo", replacement_path, '8')],
    )
    .await;
    let stale_result = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name(
                "run_shell",
                json!({"project": project_ref, "command": "pwd"}),
            )
            .unwrap(),
            Some(&alice),
        )
        .await;
    assert!(!stale_result.success);
    assert!(stale_result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("unknown_project")));
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());

    // Canonical identity deliberately addresses the currently registered
    // replacement Project, proving the legacy selector contract is unchanged.
    dispatch_shell_success(&runtime, client_id, &alice, canonical, replacement_path).await;
}

#[tokio::test]
async fn stale_project_ref_never_retargets_and_survives_store_reopen() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("refs.db");
    let canonical = "agent:special:repo";
    let original_ref = {
        let runtime = runtime_with_reference_db(&db_path);
        register_agent_projects(
            &runtime,
            "special",
            None,
            RunnerCapabilities::default(),
            vec![project("special", "repo", "repo", "/srv/original", '5')],
        )
        .await;
        let resolved = runtime.resolve_project_input(canonical).await.unwrap();
        runtime
            .project_reference_for_resolved(&resolved, None)
            .unwrap()
    };

    let runtime = runtime_with_reference_db(&db_path);
    register_agent_projects(
        &runtime,
        "special",
        None,
        RunnerCapabilities::default(),
        vec![project("special", "repo", "repo", "/srv/original", '5')],
    )
    .await;
    assert_eq!(
        runtime
            .resolve_project_input(&original_ref)
            .await
            .unwrap()
            .resolved_id,
        canonical,
        "store reopen must preserve the original mapping"
    );

    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        "special",
        "inst-special",
        Vec::new(),
    )
    .await;
    assert!(
        runtime.resolve_project_input(&original_ref).await.is_err(),
        "disappearing Project must make its old ref fail closed"
    );

    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        "special",
        "inst-special",
        vec![project("special", "repo", "repo", "/srv/replacement", '6')],
    )
    .await;
    assert!(
        runtime.resolve_project_input(&original_ref).await.is_err(),
        "same canonical id with a different root must not retarget the old ref"
    );
    assert_eq!(
        runtime
            .resolve_project_input(canonical)
            .await
            .unwrap()
            .resolved_id,
        canonical,
        "canonical full id must continue to address the replacement Project explicitly"
    );
    let replacement = runtime.resolve_project_input(canonical).await.unwrap();
    let replacement_ref = runtime
        .project_reference_for_resolved(&replacement, None)
        .unwrap();
    assert_ne!(replacement_ref, original_ref);
    assert_eq!(
        runtime
            .resolve_project_input(&replacement_ref)
            .await
            .unwrap()
            .resolved_id,
        canonical
    );
}

use super::support::*;
use crate::auth::scopes::{SCOPE_PROJECT_READ, SCOPE_RUNTIME_READ};
use crate::tool_runtime::session_context::workflow_session_authority_fingerprint;
use crate::tool_runtime::sessions::SessionCreateOptions;
use crate::tool_runtime::{registered_tool_specs, ToolCall, ToolRuntime};
use webcodex_core::workflow_session_contract::SessionMode;
use webcodex_tool_contracts::SessionLifecycleInput;

fn discovery_call(project: &str, offset: usize, limit: usize) -> ToolCall {
    ToolCall::ListSessions {
        project: project.to_string(),
        lifecycle: None,
        offset: Some(offset),
        limit: Some(limit),
    }
}

fn seed(
    runtime: &ToolRuntime,
    project: &str,
    auth: &crate::auth::AuthContext,
    title: &str,
) -> String {
    runtime
        .sessions
        .start_session_with_options(
            SessionCreateOptions::new(
                Some(project.to_string()),
                Some(title.to_string()),
                SessionMode::Normal,
                Default::default(),
            )
            .with_owner_authority_fingerprint(Some(
                workflow_session_authority_fingerprint(Some(auth)).unwrap(),
            )),
        )
        .unwrap()
        .session_id
}

#[tokio::test]
async fn session_discovery_requires_project_visibility_and_scopes() {
    let runtime = ToolRuntime::new_for_tests();
    let root = tempfile::tempdir().unwrap();
    let project =
        register_runner_project_at_path(&runtime, "discovery-owner", "demo", root.path()).await;
    let bootstrap = auth_context(None, true);
    seed(&runtime, &project, &bootstrap, "private title");
    let mut unprivileged = auth_context(Some("outsider"), false);
    unprivileged.scopes = vec![
        SCOPE_RUNTIME_READ.to_string(),
        SCOPE_PROJECT_READ.to_string(),
    ];
    let denied = runtime
        .dispatch_with_auth(discovery_call(&project, 0, 10), Some(&unprivileged))
        .await;
    assert!(!denied.success);
    assert!(denied.output.get("total").is_none());
    assert!(denied.output.get("sessions").is_none());
    let unknown = runtime
        .dispatch_with_auth(
            discovery_call("agent:missing:demo", 0, 10),
            Some(&bootstrap),
        )
        .await;
    assert!(!unknown.success);
    assert!(unknown.output.get("total").is_none());
    unprivileged.scopes = vec![SCOPE_RUNTIME_READ.to_string()];
    let scope_denied = runtime
        .dispatch_with_auth(discovery_call(&project, 0, 10), Some(&unprivileged))
        .await;
    assert!(!scope_denied.success);
    assert!(scope_denied.output.get("sessions").is_none());
}

#[tokio::test]
async fn session_discovery_returns_exact_refs_and_does_not_resume() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests().with_project_reference_database(
        std::sync::Arc::new(crate::Database::open(&root.path().join("refs.db")).unwrap()),
    );
    let project = register_runner_project_at_path(&runtime, "discovery", "demo", root.path()).await;
    let auth = auth_context(None, true);
    let foreign = auth_context(Some("foreign"), false);
    for _ in 0..22 {
        seed(&runtime, &project, &foreign, "FOREIGN TITLE");
    }
    let active = seed(&runtime, &project, &auth, "existing work");
    let closed = seed(&runtime, &project, &auth, "finished work");
    runtime.sessions.close_session(&closed).unwrap();
    let count_before = runtime.sessions.status().retained_sessions;
    let first = runtime
        .dispatch_with_auth(discovery_call(&project, 0, 1), Some(&auth))
        .await;
    assert!(first.success, "{:?}", first.error);
    assert_eq!(first.output["total"], 2);
    assert_eq!(first.output["next_offset"], 1);
    let second = runtime
        .dispatch_with_auth(discovery_call(&project, 1, 1), Some(&auth))
        .await;
    assert!(second.success);
    assert!(second.output["next_offset"].is_null());
    assert_ne!(
        first.output["sessions"][0]["session_id"],
        second.output["sessions"][0]["session_id"]
    );
    for result in [&first, &second] {
        assert!(!result.output.to_string().contains("FOREIGN TITLE"));
        assert!(serde_json::to_vec(&result.output).unwrap().len() <= 32 * 1024);
        let row = &result.output["sessions"][0];
        let reference = row["session_ref"].as_str().unwrap();
        assert_eq!(
            runtime
                .canonicalize_explicit_session_selector(reference, Some(&auth))
                .unwrap(),
            row["session_id"].as_str().unwrap()
        );
        assert!(runtime
            .canonicalize_explicit_session_selector(reference, Some(&foreign))
            .is_err());
    }
    assert_eq!(runtime.sessions.status().retained_sessions, count_before);
    assert_eq!(
        runtime
            .sessions
            .summary(&active, Some(0))
            .unwrap()
            .events_total,
        0
    );
    let historical = runtime
        .dispatch_with_auth(
            ToolCall::ListSessions {
                project: project.clone(),
                lifecycle: Some(SessionLifecycleInput::Closed),
                offset: None,
                limit: None,
            },
            Some(&auth),
        )
        .await;
    assert_eq!(historical.output["sessions"][0]["session_id"], closed);
    for _ in 0..25 {
        seed(&runtime, &project, &auth, &"界".repeat(1000));
    }
    let capped = runtime
        .dispatch_with_auth(discovery_call(&project, 0, usize::MAX), Some(&auth))
        .await;
    assert!(capped.success, "{:?}", capped.error);
    assert_eq!(capped.output["sessions"].as_array().unwrap().len(), 20);
    assert_eq!(capped.output["next_offset"], 20);
    assert!(serde_json::to_vec(&capped.output).unwrap().len() <= 32 * 1024);
    assert!(capped.output["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["title"]
            .as_str()
            .is_none_or(|title| title.chars().count() <= 240)));
}

#[test]
fn session_discovery_is_model_visible_and_has_closed_input_schema() {
    let specs = registered_tool_specs();
    let spec = specs
        .iter()
        .find(|spec| spec.name == "list_sessions")
        .unwrap();
    assert_eq!(
        spec.input_schema["required"],
        serde_json::json!(["project"])
    );
    let call = serde_json::from_value::<ToolCall>(serde_json::json!({
        "tool": "list_sessions", "params": {"project": "agent:demo:project", "lifecycle": "unknown"}
    }));
    assert!(call.is_err());
    assert!(spec.output_schema.is_object());
}

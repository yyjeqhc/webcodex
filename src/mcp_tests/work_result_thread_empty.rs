use super::super::tools::{
    handle_call, work_result_thread_binding_for_test, WORK_RESULT_APP_RESULT_META_KEY,
    WORK_RESULT_THREAD_CONTEXT_META_KEY,
};
use super::*;
use crate::client_window::ClientWindow;
use std::sync::Arc;

async fn open_panel(
    runtime: &ToolRuntime,
    auth: Option<&AuthContext>,
    window: Option<&ClientWindow>,
    apps_enabled: bool,
    stateless: bool,
) -> McpOutcome {
    handle_call(
        runtime,
        json!({"name": "work_result_thread_panel", "arguments": {}}),
        Some(json!(1)),
        auth,
        stateless,
        false,
        apps_enabled,
        HostFileImportTrust::Untrusted,
        window,
        None,
        None,
        None,
    )
    .await
}

#[test]
fn work_result_thread_empty_schema_is_launcher_only() {
    let listed = mcp_tools_list_payload_with_features_for_auth(false, true, true, None);
    let launcher = listed["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "work_result_thread_panel")
        .unwrap();
    assert_eq!(
        launcher["outputSchema"]["properties"]["output"]["properties"]["work_result"]["type"],
        json!(["object", "null"])
    );
    let state = crate::tool_runtime::work_result_app_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "get_work_result_state")
        .unwrap();
    assert_eq!(
        state.output_schema["properties"]["output"]["properties"]["work_result"]["type"],
        "object"
    );
}

#[tokio::test]
async fn work_result_thread_empty_panel_requires_scope_and_protocol_admission() {
    let temp = tempfile::tempdir().unwrap();
    let db = Arc::new(crate::Database::open(&temp.path().join("thread-empty.db")).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_window_activity_database(db.clone());
    let mut auth = crate::auth::shared_key_context("thread-empty-scope");
    auth.scopes = vec![crate::auth::SCOPE_RUNTIME_READ.into()];
    let window = ClientWindow::for_test("thread-empty-scope-window");
    assert!(matches!(
        open_panel(&runtime, Some(&auth), Some(&window), true, true).await,
        McpOutcome::Forbidden {
            required_scope: Some(crate::auth::SCOPE_PROJECT_READ),
            ..
        }
    ));

    auth.scopes.push(crate::auth::SCOPE_PROJECT_READ.into());
    for (enabled, stateless) in [(false, true), (true, false)] {
        assert!(matches!(
            open_panel(&runtime, Some(&auth), Some(&window), enabled, stateless).await,
            McpOutcome::BadRequest(_)
        ));
    }
    let McpOutcome::Ok(empty) = open_panel(&runtime, Some(&auth), Some(&window), true, true).await
    else {
        panic!("authorized unbound Window must open an empty panel")
    };
    assert_eq!(
        empty["result"]["structuredContent"],
        json!({"success": true, "output": {"work_result": null}, "error": null})
    );
    assert_eq!(empty["result"]["isError"], false);
    assert_eq!(empty["result"]["resultType"], "complete");
    assert_eq!(
        empty["result"]["_meta"][WORK_RESULT_APP_RESULT_META_KEY],
        empty["result"]["structuredContent"]
    );
    assert_eq!(
        empty["result"]["_meta"][WORK_RESULT_THREAD_CONTEXT_META_KEY],
        json!({"empty": true, "session_id": null})
    );
    assert!(!empty["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .trim_start()
        .starts_with('{'));
    assert!(
        work_result_thread_binding_for_test(&runtime, Some(&auth), Some(&window))
            .unwrap()
            .is_none()
    );
    assert!(db
        .list_window_activity_events(window.key(), None, 20)
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn work_result_thread_empty_panel_does_not_mask_identity_or_store_failures() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("thread-empty-errors.db");
    let db = Arc::new(crate::Database::open(&path).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_window_activity_database(db);
    let auth = crate::auth::shared_key_context("thread-empty-errors");
    let anonymous = crate::auth::open_anonymous_context();
    let window = ClientWindow::for_test("thread-empty-errors-window");
    for (auth, window) in [
        (Some(&auth), None),
        (None, Some(&window)),
        (Some(&anonymous), Some(&window)),
    ] {
        let McpOutcome::BadRequest(failure) = open_panel(&runtime, auth, window, true, true).await
        else {
            panic!("missing stable identity must fail closed")
        };
        assert_eq!(failure["error"]["code"], -32602);
        assert!(failure.get("result").is_none());
    }

    let runtime_without_store = ToolRuntime::new_for_tests();
    assert!(matches!(
        open_panel(
            &runtime_without_store,
            Some(&auth),
            Some(&window),
            true,
            true
        )
        .await,
        McpOutcome::BadRequest(_)
    ));

    // Corrupt only this disposable fixture's query schema. A failed lookup is
    // distinct from a successful lookup that found no presentation binding.
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("ALTER TABLE action_events RENAME TO unreadable_action_events")
        .unwrap();
    let McpOutcome::BadRequest(failure) =
        open_panel(&runtime, Some(&auth), Some(&window), true, true).await
    else {
        panic!("activity lookup failure must not become an empty panel")
    };
    assert_eq!(failure["error"]["code"], -32602);
    assert!(failure.get("result").is_none());
}

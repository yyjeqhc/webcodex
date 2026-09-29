use super::*;

#[tokio::test]
async fn console_job_query_uses_the_same_authorized_inventory_without_model_only_fields() {
    let runtime = test_runtime();
    let auth = crate::auth::shared_key_context("console-job-query");
    let foreign = crate::auth::shared_key_context("console-job-query-foreign");
    register_project(
        &runtime,
        "job-query-own",
        "demo",
        "/private/own",
        Some(&auth),
    )
    .await;
    register_project(
        &runtime,
        "job-query-foreign",
        "demo",
        "/private/foreign",
        Some(&foreign),
    )
    .await;
    let project = "agent:job-query-own:demo";
    let session = start_authorized_session(&runtime, project, &auth);
    let started = runtime
        .dispatch_with_auth(
            ToolCall::RunJob {
                project: project.into(),
                command: "echo PRIVATE_COMMAND".into(),
                session_id: Some(session.session_id.clone()),
                timeout_secs: None,
                cwd: None,
                purpose: None,
                shell: None,
            },
            Some(&auth),
        )
        .await;
    assert!(started.success, "{:?}", started.error);
    let before = runtime
        .sessions
        .summary(&session.session_id, Some(200))
        .unwrap();
    let canonical = runtime
        .list_jobs_for_auth_with_filters(
            Some(100),
            None,
            Some(project.into()),
            Some(session.session_id.clone()),
            Some(&auth),
        )
        .await;
    assert!(canonical.success);
    let (console, truncated) = session_jobs_for_auth(&runtime, &auth, project, &session.session_id)
        .await
        .unwrap();
    assert_eq!(truncated, canonical.output["truncated"].as_bool().unwrap());
    assert_eq!(console.len(), 1);
    let wire = serde_json::to_value(&console[0]).unwrap();
    let expected = &canonical.output["jobs"][0];
    for key in ["job_id", "kind", "status", "created_at"] {
        assert_eq!(wire[key], expected[key], "{key}");
    }
    assert_eq!(wire["terminal"], false);
    for private in [
        "PRIVATE",
        "project",
        "validation",
        "command",
        "executor",
        "source_state",
    ] {
        assert!(
            !wire.to_string().contains(private),
            "leaked {private}: {wire}"
        );
    }
    let (invisible, truncated) =
        session_jobs_for_auth(&runtime, &foreign, project, &session.session_id)
            .await
            .unwrap();
    assert!(invisible.is_empty());
    assert!(!truncated);
    let denied_counts = running_jobs_for_auth(&runtime, &foreign, Some(project))
        .await
        .unwrap();
    assert_eq!(denied_counts.count(project, &session.session_id), 0);
    assert_eq!(
        runtime
            .sessions
            .summary(&session.session_id, Some(200))
            .unwrap()
            .events_total,
        before.events_total
    );
}

use super::jobs::{register_job_agent_for_auth, start_agent_runtime_job_in_session};
use super::support::*;
use serde_json::json;

#[tokio::test]
async fn typed_job_query_matches_inventory_order_bounds_and_does_not_select_foreign_jobs() {
    let runtime = test_runtime();
    let auth = shared_key_auth_context("typed-query-own");
    let foreign = shared_key_auth_context("typed-query-foreign");
    register_job_agent_for_auth(&runtime, "typed-own", "demo", &auth).await;
    register_job_agent_for_auth(&runtime, "typed-foreign", "demo", &foreign).await;
    let project = "agent:typed-own:demo";
    let session = runtime.sessions.start_session(Some(project.into()), None);
    for _ in 0..3 {
        start_agent_runtime_job_in_session(
            &runtime,
            "typed-own",
            "demo",
            Some(&session.session_id),
            &auth,
        )
        .await;
    }
    start_agent_runtime_job_in_session(&runtime, "typed-foreign", "demo", None, &foreign).await;
    let before = runtime
        .sessions
        .summary(&session.session_id, Some(200))
        .unwrap();
    let page = runtime
        .query_job_inventory_for_auth(
            Some(2),
            None,
            Some(project),
            Some(&session.session_id),
            Some(&auth),
        )
        .await
        .unwrap();
    assert_eq!(page.matched_count, 3);
    assert_eq!(page.jobs.len(), 2);
    assert!(page.truncated());
    let model = runtime
        .list_jobs_for_auth_with_filters(
            Some(2),
            None,
            Some(project.into()),
            Some(session.session_id.clone()),
            Some(&auth),
        )
        .await;
    assert!(model.success);
    assert_eq!(model.output["matched_count"], page.matched_count);
    assert_eq!(model.output["truncated"], page.truncated());
    assert_eq!(
        model.output["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|job| job["job_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        page.jobs
            .iter()
            .map(|job| job.job_id.as_str())
            .collect::<Vec<_>>()
    );
    assert!(page
        .jobs
        .iter()
        .all(|job| job.project_id.as_deref() == Some(project)
            && job.session_id.as_deref() == Some(session.session_id.as_str())));
    let denied = runtime
        .query_job_inventory_for_auth(
            None,
            None,
            Some("agent:typed-foreign:demo"),
            None,
            Some(&auth),
        )
        .await
        .unwrap();
    assert!(denied.jobs.is_empty());
    assert_eq!(denied.matched_count, 0);
    assert!(!denied.truncated());
    let after = runtime
        .sessions
        .summary(&session.session_id, Some(200))
        .unwrap();
    assert_eq!(
        before.events_total, after.events_total,
        "queries do not become Session evidence"
    );
}

#[tokio::test]
async fn typed_job_query_retains_invalid_filter_errors_and_empty_status_normalization() {
    let runtime = test_runtime();
    for (project, session, kind) in [
        (Some(" "), None, "invalid_project_filter"),
        (None, Some(" "), "invalid_session_filter"),
    ] {
        let error = runtime
            .query_job_inventory_for_auth(None, None, project, session, None)
            .await
            .err()
            .unwrap();
        let model = runtime
            .list_jobs_for_auth_with_filters(
                None,
                None,
                project.map(str::to_string),
                session.map(str::to_string),
                None,
            )
            .await;
        assert_eq!(
            serde_json::to_value(&error).unwrap(),
            serde_json::to_value(model).unwrap()
        );
        assert_eq!(error.output["error_kind"], kind);
        assert_eq!(error.output["state_changed"], false);
    }
    let empty = runtime
        .list_jobs_for_auth_with_filters(Some(0), Some("  ".into()), None, None, None)
        .await;
    assert_eq!(
        empty.output,
        json!({"jobs":[],"count":0,"matched_count":0,"truncated":false})
    );
}

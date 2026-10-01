use super::support::*;
use crate::tool_runtime::session_context::workflow_session_authority_fingerprint;
use crate::tool_runtime::{sessions, ToolCall, ToolRuntime};
use serde_json::json;
use std::sync::Arc;

fn record(project: &str, session: &str) -> ToolCall {
    ToolCall::RecordExternalObservation {
        project: project.into(),
        session_id: session.into(),
        adapter_id: "a".repeat(64),
        event_id: "b".repeat(64),
        observed_tool: "Bash".into(),
        exit_code: None,
    }
}

#[tokio::test]
async fn external_observations_runtime_scope_replay_unknown_and_readback() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(crate::db::Database::open(&tmp.path().join("db")).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_communication_database(db.clone());
    let auth = auth_context(None, true);
    let project = register_runner_project_at_path(&runtime, "external", "p", tmp.path()).await;
    let mut opts = sessions::SessionCreateOptions::new(
        Some(project.clone()),
        None,
        Default::default(),
        Default::default(),
    );
    opts.owner_authority_fingerprint =
        Some(workflow_session_authority_fingerprint(Some(&auth)).unwrap());
    let session = runtime
        .sessions
        .start_session_with_options(opts)
        .unwrap()
        .session_id;
    let before_external = runtime.sessions.summary(&session, None).unwrap();
    let result = runtime
        .dispatch_with_auth(record(&project, &session), Some(&auth))
        .await;
    assert!(result.success, "{:?}", result);
    assert_eq!(result.output["observation"]["status"], "unknown");
    assert_eq!(result.output["provenance"], "external_report");
    assert_eq!(result.output["inserted"], true);
    let replay = runtime
        .dispatch_with_auth(record(&project, &session), Some(&auth))
        .await;
    assert!(replay.success, "{:?}", replay);
    assert_eq!(replay.output["inserted"], false);
    let read = runtime
        .dispatch_with_auth(
            ToolCall::ListExternalObservations {
                project: project.clone(),
                session_id: session.clone(),
            },
            Some(&auth),
        )
        .await;
    assert!(read.success, "{:?}", read);
    assert_eq!(read.output["observations"].as_array().unwrap().len(), 1);
    assert_eq!(read.output["coverage"]["complete"], false);
    assert_eq!(
        read.output["coverage"]["reason"],
        "source_sequence_unavailable"
    );
    let after_external = runtime.sessions.summary(&session, None).unwrap();
    assert_eq!(after_external.events_total, before_external.events_total);
    assert_eq!(after_external.events.len(), before_external.events.len());
    assert_eq!(after_external.updated_at, before_external.updated_at);
    let mut conflict = record(&project, &session);
    if let ToolCall::RecordExternalObservation { exit_code, .. } = &mut conflict {
        *exit_code = Some(0);
    }
    let denied = runtime.dispatch_with_auth(conflict, Some(&auth)).await;
    assert!(!denied.success);
    assert_eq!(denied.output["error_kind"], "external_observation_conflict");
    assert_eq!(denied.output["failure_kind"], "conflict");
    assert_eq!(denied.output["state_changed"], false);
    assert_eq!(denied.output["recovery_kind"], "fix_input");

    db.conn_for_tests()
        .execute_batch(
            "CREATE TRIGGER fail_external_observation BEFORE INSERT ON wc_external_observations              BEGIN SELECT RAISE(ABORT,'injected'); END;",
        )
        .unwrap();
    let mut uncertain_call = record(&project, &session);
    if let ToolCall::RecordExternalObservation { event_id, .. } = &mut uncertain_call {
        *event_id = "c".repeat(64);
    }
    let uncertain = runtime
        .dispatch_with_auth(uncertain_call, Some(&auth))
        .await;
    assert!(!uncertain.success);
    assert_eq!(
        uncertain.output["error_kind"],
        "external_observation_storage_uncertain"
    );
    assert_eq!(uncertain.output["failure_kind"], "outcome_unknown");
    assert!(uncertain.output["state_changed"].is_null());
    assert_eq!(uncertain.output["recovery_kind"], "retry_same");
    assert_eq!(uncertain.output["retry_same_event_identity"], true);
    db.conn_for_tests()
        .execute_batch("DROP TRIGGER fail_external_observation")
        .unwrap();

    let mut retry_same = record(&project, &session);
    if let ToolCall::RecordExternalObservation { event_id, .. } = &mut retry_same {
        *event_id = "c".repeat(64);
    }
    let recovered = runtime.dispatch_with_auth(retry_same, Some(&auth)).await;
    assert!(recovered.success, "{recovered:?}");
    assert_eq!(recovered.output["inserted"], true);

    let other = register_runner_project_at_path(&runtime, "external-other", "p", tmp.path()).await;
    assert!(
        !runtime
            .dispatch_with_auth(record(&other, &session), Some(&auth))
            .await
            .success
    );
    let mut stranger = auth_context(Some("stranger"), false);
    stranger.scopes = vec!["admin".into()];
    assert!(
        !runtime
            .dispatch_with_auth(record(&project, &session), Some(&stranger))
            .await
            .success
    );
    let mut no_scope = auth_context(Some("stranger"), false);
    no_scope.scopes = vec!["project:read".into()];
    assert!(
        !runtime
            .dispatch_with_auth(record(&project, &session), Some(&no_scope))
            .await
            .success
    );
    assert_eq!(
        db.list_external_observations(&session, &project)
            .unwrap()
            .len(),
        2
    );
    // Neither accepting nor reading these reports creates a native Job receipt.
    assert!(db
        .load_job_receipts(chrono::Utc::now().timestamp())
        .unwrap()
        .is_empty());
    let closed = runtime
        .dispatch_with_auth(
            ToolCall::CloseSession {
                session_id: session.clone(),
            },
            Some(&auth),
        )
        .await;
    assert!(closed.success, "{:?}", closed);
    assert!(
        !runtime
            .dispatch_with_auth(record(&project, &session), Some(&auth))
            .await
            .success
    );
    db.conn_for_tests()
        .execute_batch("DROP TABLE wc_external_observations")
        .unwrap();
    let list_failure = runtime
        .dispatch_with_auth(
            ToolCall::ListExternalObservations {
                project: project.clone(),
                session_id: session.clone(),
            },
            Some(&auth),
        )
        .await;
    assert!(!list_failure.success);
    assert_eq!(
        list_failure.output["error_kind"],
        "external_observation_store_unavailable"
    );
    assert_eq!(list_failure.output["state_changed"], false);
    assert_eq!(list_failure.output["recovery_kind"], "reobserve");
    assert!(list_failure
        .output
        .get("retry_same_event_identity")
        .is_none());
}

#[tokio::test]
async fn external_reports_reach_default_and_diagnostic_handoff_without_native_promotion() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(crate::db::Database::open(&tmp.path().join("db")).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_communication_database(db.clone());
    let auth = auth_context(None, true);
    let project =
        register_runner_project_at_path(&runtime, "handoff-external", "p", tmp.path()).await;
    let mut opts = sessions::SessionCreateOptions::new(
        Some(project.clone()),
        None,
        Default::default(),
        Default::default(),
    );
    opts.owner_authority_fingerprint =
        Some(workflow_session_authority_fingerprint(Some(&auth)).unwrap());
    let session = runtime
        .sessions
        .start_session_with_options(opts)
        .unwrap()
        .session_id;
    let before = runtime.sessions.summary(&session, None).unwrap();
    let baseline = runtime
        .session_handoff_summary(
            session.clone(),
            Some(project.clone()),
            Some(false),
            Some(false),
            Some(true),
            true,
            Some(20),
            Some(&auth),
        )
        .await;
    assert!(baseline.success, "{baseline:?}");
    for (n, exit_code) in [
        (0, None),
        (1, Some(0)),
        (2, Some(9)),
        (3, None),
        (4, Some(0)),
        (5, None),
        (6, None),
    ] {
        db.record_external_observation(
            &session,
            &project,
            webcodex_store::ExternalObservation {
                adapter_id: "a".repeat(64),
                event_id: format!("{n:064x}"),
                tool: "Bash".into(),
                exit_code,
                recorded_at: n + 1,
            },
        )
        .unwrap();
    }
    let call = |diagnostic| ToolCall::SessionHandoffSummary {
        session_id: session.clone(),
        project: Some(project.clone()),
        include_workspace: Some(false),
        include_checkpoints: Some(false),
        include_validation: Some(true),
        diagnostic,
        limit: Some(20),
    };
    let after_external = runtime
        .session_handoff_summary(
            session.clone(),
            Some(project.clone()),
            Some(false),
            Some(false),
            Some(true),
            true,
            Some(20),
            Some(&auth),
        )
        .await;
    assert!(after_external.success, "{after_external:?}");
    for pointer in [
        "/handoff_brief/progress",
        "/handoff_brief/validation",
        "/task_outcome",
    ] {
        assert_eq!(
            after_external.output.pointer(pointer),
            baseline.output.pointer(pointer),
            "{pointer}"
        );
    }
    let compact = runtime.dispatch_with_auth(call(false), Some(&auth)).await;
    assert!(compact.success, "{compact:?}");
    let handoff_spec = crate::tool_runtime::registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "read_session_handoff")
        .unwrap();
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &json!({"success": true, "output": compact.output.clone()}),
        &handoff_spec.output_schema,
    )
    .unwrap();
    let reports = &compact.output["handoff_brief"]["external_observations"];
    assert_eq!(reports["status"], "available");
    assert_eq!(reports["provenance"], "external_report");
    assert_eq!(reports["coverage"]["complete"], false);
    assert_eq!(reports["coverage"]["reason"], "source_sequence_unavailable");
    assert_eq!(reports["total"], 7);
    assert_eq!(reports["returned"], 5);
    assert_eq!(reports["truncated"], true);
    assert_eq!(reports["unknown_count"], 4);
    assert_eq!(
        reports["observations"][0]["event_id"],
        format!("{:064x}", 2)
    );
    assert_eq!(reports["observations"][0]["status"], "reported_failure");
    assert_eq!(reports["observations"][2]["status"], "reported_success");
    assert_eq!(
        reports["observations"][4]["event_id"],
        format!("{:064x}", 6)
    );
    assert_eq!(reports["observations"][4]["status"], "unknown");
    assert_eq!(compact.output["project"], project);
    assert_eq!(
        compact.output["handoff_brief"]["session"]["session_id"],
        session
    );
    assert_ne!(
        compact.output["handoff_brief"]["validation"]["status"],
        "passed"
    );
    let diagnostic = runtime.dispatch_with_auth(call(true), Some(&auth)).await;
    assert!(diagnostic.success, "{diagnostic:?}");
    assert_eq!(
        diagnostic.output["handoff_brief"]["external_observations"],
        *reports
    );
    // Only normal handoff-tool telemetry enters the native Session ledger.
    let after = runtime.sessions.summary(&session, None).unwrap();
    assert_eq!(after.events_total, before.events_total + 4);
    assert!(after.events[before.events.len()..]
        .iter()
        .all(|event| event.tool_name == "read_session_handoff"));

    let other =
        register_runner_project_at_path(&runtime, "handoff-external-other", "p", tmp.path()).await;
    let mut wrong_call = call(false);
    if let ToolCall::SessionHandoffSummary {
        project: requested, ..
    } = &mut wrong_call
    {
        *requested = Some(other);
    }
    let wrong_project = runtime.dispatch_with_auth(wrong_call, Some(&auth)).await;
    assert!(!wrong_project.success);
    assert!(wrong_project.output.get("handoff_brief").is_none());
    let mut stranger = auth_context(Some("stranger"), false);
    stranger.scopes = vec!["admin".into()];
    let denied = runtime
        .dispatch_with_auth(call(false), Some(&stranger))
        .await;
    assert!(!denied.success);
    assert!(denied.output.get("handoff_brief").is_none());
}

#[tokio::test]
async fn external_handoff_read_distinguishes_empty_missing_store_and_failed_read() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(crate::db::Database::open(&tmp.path().join("db")).unwrap());
    let with_db = ToolRuntime::new_for_tests().with_communication_database(db.clone());
    let project = register_runner_project_at_path(&with_db, "handoff-empty", "p", tmp.path()).await;
    let session = with_db
        .sessions
        .start_session(Some(project.clone()), None)
        .session_id;
    let handoff =
        |runtime: &ToolRuntime| runtime.handoff_external_observations(&session, Some(&project));
    let empty = handoff(&with_db);
    assert_eq!(empty["status"], "available");
    assert_eq!(empty["total"], 0);
    assert_eq!(empty["observations"], json!([]));
    assert_eq!(empty["coverage"]["complete"], false);
    let empty_handoff = with_db
        .session_handoff_summary(
            session.clone(),
            Some(project.clone()),
            Some(false),
            Some(false),
            Some(false),
            true,
            Some(20),
            None,
        )
        .await;
    assert!(empty_handoff.success, "{empty_handoff:?}");
    assert_eq!(
        empty_handoff.output["handoff_brief"]["external_observations"],
        empty
    );
    let missing_project = with_db.handoff_external_observations(&session, None);
    assert_eq!(missing_project["status"], "unavailable");
    assert_eq!(
        missing_project["reason_code"],
        "session_project_unavailable"
    );
    assert!(missing_project["observations"].is_null());
    let no_db = ToolRuntime::new_for_tests();
    let missing_store = handoff(&no_db);
    assert_eq!(missing_store["reason_code"], "store_unavailable");
    assert!(missing_store["total"].is_null());
    let no_db_project =
        register_runner_project_at_path(&no_db, "handoff-no-db", "p", tmp.path()).await;
    let no_db_session = no_db
        .sessions
        .start_session(Some(no_db_project.clone()), None)
        .session_id;
    let no_db_handoff = no_db
        .session_handoff_summary(
            no_db_session,
            Some(no_db_project),
            Some(false),
            Some(false),
            Some(false),
            false,
            Some(20),
            None,
        )
        .await;
    assert!(no_db_handoff.success, "{no_db_handoff:?}");
    assert_eq!(
        no_db_handoff.output["handoff_brief"]["external_observations"]["status"],
        "unavailable"
    );
    assert!(
        no_db_handoff.output["handoff_brief"]["external_observations"]["observations"].is_null()
    );
    db.conn_for_tests()
        .execute_batch("DROP TABLE wc_external_observations")
        .unwrap();
    let failed_read = handoff(&with_db);
    assert_eq!(failed_read["status"], "unavailable");
    assert_eq!(failed_read["reason_code"], "store_unavailable");
    assert_eq!(failed_read["coverage"]["complete"], false);
    assert!(failed_read["observations"].is_null());
    let failed_handoff = with_db
        .session_handoff_summary(
            session,
            Some(project),
            Some(false),
            Some(false),
            Some(false),
            true,
            Some(20),
            None,
        )
        .await;
    assert!(failed_handoff.success, "{failed_handoff:?}");
    assert_eq!(
        failed_handoff.output["handoff_brief"]["external_observations"],
        failed_read
    );
    assert_eq!(
        failed_handoff.output["task_outcome"],
        empty_handoff.output["task_outcome"]
    );
}

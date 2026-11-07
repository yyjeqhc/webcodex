//! Recovery reads must preserve concurrent task corrections and execution identity.

use super::*;

#[tokio::test]
async fn acceptance_is_rederived_from_restored_evidence_without_recording_or_execution() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("sessions.json");
    let runtime = test_runtime().with_session_ledger(&ledger);
    let session = runtime
        .sessions
        .start_session(None, Some("Review the task".into()));
    record_handoff_tool_event(
        &runtime,
        &session.session_id,
        "cargo_check",
        json!({}),
        false,
        json!({"exit_code": 1}),
    );
    let read = |runtime: ToolRuntime, sid: String| async move {
        runtime
            .session_handoff_summary(
                sid,
                None,
                Some(false),
                Some(false),
                Some(false),
                false,
                Some(20),
                None,
            )
            .await
    };
    let original = read(runtime.clone(), session.session_id.clone()).await;
    assert!(original.success, "{original:?}");
    assert_ne!(original.output["acceptance"]["state"], "ready_for_review");
    runtime.sessions.flush_persistence();
    drop(runtime);
    let restored = test_runtime().with_session_ledger(&ledger);
    let before = restored
        .sessions
        .summary(&session.session_id, None)
        .unwrap();
    let recovered = read(restored.clone(), session.session_id.clone()).await;
    assert!(recovered.success, "{recovered:?}");
    assert_eq!(
        recovered.output["acceptance"],
        original.output["acceptance"]
    );
    let after = restored
        .sessions
        .summary(&session.session_id, None)
        .unwrap();
    assert_eq!(before.events_total, after.events_total);
    assert_eq!(
        serde_json::to_value(before.events).unwrap(),
        serde_json::to_value(after.events).unwrap()
    );
}

#[tokio::test]
async fn handoff_reread_recovers_corrected_task_without_reusing_validation_or_restarting_job() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    commit_file(root.path(), "README.md", "initial\n", "initial");
    let runtime = test_runtime();
    let client = "handoff-corrected-task";
    let auth = auth_context(None, true);
    let project =
        register_runner_project_at_path_with_auth(&runtime, client, "demo", root.path(), &auth)
            .await;
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("Implement the original request".into()),
    );
    let sid = session.session_id;
    let instruction = |text: &str| {
        runtime
            .sessions
            .ensure_coding_session(sessions::CodingSessionRequest {
                project_identity: None,
                project: project.clone(),
                authority_fingerprint: sessions::TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT
                    .to_string(),
                resume_session_id: Some(sid.clone()),
                instruction: Some(text.into()),
                mode: SessionMode::Normal,
                guards: SessionGuards::default(),
                execution_context: None,
                project_instructions: None,
                transport: SessionTransport::Api,
                context_refreshed: true,
                write_scope_verified: true,
            })
            .unwrap();
    };
    instruction("Implement the original request");
    post_session_message(&runtime, &sid, "todo", "Implement the original scope");
    let job_id =
        seed_session_projection_job(&runtime, client, &project, &sid, "running", "", &auth).await;
    record_handoff_tool_event(
        &runtime,
        &sid,
        "cargo_check",
        json!({"project": project}),
        true,
        json!({"exit_code": 0}),
    );
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let sid = sid.clone();
        async move {
            runtime
                .session_handoff_summary(
                    sid,
                    None,
                    Some(true),
                    Some(false),
                    Some(true),
                    true,
                    Some(20),
                    None,
                )
                .await
        }
    });
    // This pending workspace request is the deterministic boundary after the
    // initial task/notes/validation snapshots, before handoff finishes.
    let request = wait_for_patch_agent_request(&runtime, client).await;
    instruction("User correction: preserve the external writer's changes");
    post_session_message(
        &runtime,
        &sid,
        "decision",
        "Keep the user's corrected scope",
    );
    post_session_message(
        &runtime,
        &sid,
        "progress",
        "Implementation remains; validate the corrected source",
    );
    record_handoff_tool_event(
        &runtime,
        &sid,
        "edit_project_files",
        json!({"project": project, "changes": [{"kind": "edit", "path": "README.md"}]}),
        true,
        json!({"state_changed": true}),
    );
    let external_bytes = "external writer's correction\n";
    std::fs::write(root.path().join("README.md"), external_bytes).unwrap();
    complete_agent_request_by_running_locally(&runtime, client, request).await;
    let raced = task.await.unwrap();
    assert!(raced.success, "{raced:?}");
    let brief = &raced.output["handoff_brief"];
    assert_reason_list_contains(
        &brief["basis"],
        "reason_codes",
        "session_changed_during_snapshot",
    );
    assert_eq!(brief["basis"]["complete"], false);
    assert_eq!(brief["progress"]["state"], "insufficient_evidence");
    assert_eq!(
        brief["task"]["latest_instruction"]["excerpt"],
        "Implement the original request"
    );
    assert_eq!(brief["task"]["decisions"]["returned"], 0);
    assert_eq!(raced.output["validation"]["latest_status"], "passed");
    assert_eq!(
        brief["next_actions"][0],
        "re-read read_session_handoff for the exact Session before dependent work"
    );
    let recovery_actions = brief["next_actions"].clone();
    assert_eq!(
        json!({
            "top_level": raced.output["suggested_next_actions"],
            "attempt": raced.output["continuation_feedback"]["attempt"]["suggested_next_actions"],
        }),
        json!({
            "top_level": recovery_actions,
            "attempt": recovery_actions,
        }),
        "a diagnostic race must expose only the exact handoff recovery action"
    );
    assert!(raced.output.get("verdict").is_none());
    assert!(raced.output.get("task_outcome").is_none());
    assert_eq!(raced.output["jobs"]["recent"][0]["job_id"], job_id);
    assert_eq!(raced.output["facts"]["active_jobs"]["active_count"], 1);
    assert!(serde_json::to_vec(brief).unwrap().len() < 8 * 1024);
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &serde_json::to_value(&raced).unwrap(),
        &webcodex_tool_contracts::output_schema_for_tool("read_session_handoff"),
    )
    .expect("raced diagnostic handoff must preserve its registered schema");

    let before = runtime.sessions.summary(&sid, Some(200)).unwrap();
    let refreshed =
        dispatch_handoff_diagnostic_with_agent(&runtime, client, sid.clone(), None, true, false)
            .await;
    assert!(refreshed.success, "{refreshed:?}");
    let brief = &refreshed.output["handoff_brief"];
    assert_eq!(brief["basis"]["complete"], true);
    assert!(refreshed.output.get("verdict").is_some());
    assert!(refreshed.output.get("task_outcome").is_some());
    assert_eq!(
        brief["task"]["root_instruction"]["excerpt"],
        "Implement the original request"
    );
    assert_eq!(
        brief["task"]["latest_instruction"]["excerpt"],
        "User correction: preserve the external writer's changes"
    );
    assert_eq!(
        brief["task"]["decisions"]["items"][0]["excerpt"],
        "Keep the user's corrected scope"
    );
    assert_eq!(
        brief["task"]["recent_progress"]["items"][0]["excerpt"],
        "Implementation remains; validate the corrected source"
    );
    // The corrected instruction starts a new attempt. The old passing result
    // remains historical, while no validation proves the corrected task.
    assert_eq!(brief["validation"]["status"], "not_run");
    assert_eq!(refreshed.output["validation"]["latest_status"], "passed");
    assert_eq!(brief["attention"]["active_jobs"], 1);
    assert_eq!(refreshed.output["jobs"]["recent"][0]["job_id"], job_id);
    assert_eq!(refreshed.output["jobs"]["running_count"], 1);
    assert_eq!(
        std::fs::read_to_string(root.path().join("README.md")).unwrap(),
        external_bytes
    );
    let after = runtime.sessions.summary(&sid, Some(200)).unwrap();
    // The public read records its standard observation telemetry only; it
    // cannot add another instruction or execute a replacement Job.
    let added_events = after
        .events
        .iter()
        .filter(|event| {
            !before
                .events
                .iter()
                .any(|prior| prior.event_id == event.event_id)
        })
        .collect::<Vec<_>>();
    assert_eq!(added_events.len(), 2);
    assert!(added_events.iter().all(|event| {
        event.tool_name == "read_session_handoff" && !event.write_like && !event.shell_like
    }));
    assert!(
        probe_patch_agent_request(&runtime, client).await.is_none(),
        "recovery must not redispatch the running Job"
    );
}

#[tokio::test]
async fn handoff_diagnostic_external_race_omits_completion_verdict_but_retains_validation() {
    let root = tempfile::tempdir().unwrap();
    let db_root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    commit_file(root.path(), "README.md", "initial\n", "initial");
    let db = std::sync::Arc::new(crate::db::Database::open(&db_root.path().join("db")).unwrap());
    let runtime = test_runtime().with_communication_database(db.clone());
    let client = "handoff-diagnostic-external-race";
    let project = register_runner_project_at_path(&runtime, client, "demo", root.path()).await;
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let sid = session.session_id;
    record_handoff_tool_event(
        &runtime,
        &sid,
        "cargo_check",
        json!({"project": project}),
        true,
        json!({"exit_code": 0}),
    );
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let sid = sid.clone();
        async move {
            runtime
                .session_handoff_summary(
                    sid,
                    None,
                    Some(true),
                    Some(false),
                    Some(true),
                    true,
                    Some(20),
                    None,
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, client).await;
    db.record_external_observation(
        &sid,
        &project,
        webcodex_store::ExternalObservation {
            adapter_id: "a".repeat(64),
            event_id: "b".repeat(64),
            tool: "Bash".to_string(),
            exit_code: None,
            recorded_at: 1,
        },
    )
    .unwrap();
    complete_agent_request_by_running_locally(&runtime, client, request).await;
    let raced = task.await.unwrap();
    assert!(raced.success, "{raced:?}");
    let brief = &raced.output["handoff_brief"];
    assert_reason_list_contains(
        &brief["basis"],
        "reason_codes",
        "external_observations_changed_during_snapshot",
    );
    assert_eq!(brief["basis"]["complete"], false);
    assert_eq!(brief["progress"]["state"], "insufficient_evidence");
    assert_eq!(raced.output["validation"]["latest_status"], "passed");
    assert_eq!(raced.output["facts"]["validations_passed"], 1);
    assert_eq!(brief["validation"]["status"], "inconclusive");
    assert_eq!(
        brief["validation"]["reason_code"],
        "validation_inconclusive"
    );
    assert!(raced.output.get("verdict").is_none());
    assert!(raced.output.get("task_outcome").is_none());
    assert_eq!(
        raced.output["suggested_next_actions"],
        brief["next_actions"]
    );
    assert_eq!(
        raced.output["continuation_feedback"]["attempt"]["suggested_next_actions"],
        brief["next_actions"]
    );
    assert!(serde_json::to_vec(brief).unwrap().len() < 8 * 1024);
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &serde_json::to_value(&raced).unwrap(),
        &webcodex_tool_contracts::output_schema_for_tool("read_session_handoff"),
    )
    .expect("external-raced diagnostic handoff must preserve its registered schema");
    assert!(probe_patch_agent_request(&runtime, client).await.is_none());
}


#[tokio::test]
async fn work_on_project_without_session_id_always_creates_fresh_session() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_runner_project_at_path(&runtime, "wop-create", "demo", root.path()).await;
    let auth = auth_context(None, true);

    let result = dispatch_coding_call_in_window(
        &runtime,
        "wop-create",
        work_on_project_call("demo", "first root instruction", None),
        Some(&auth),
        "wop-create-window",
    )
    .await;
    assert!(result.success, "{:?}", result.error);

    // Compact projection keeps identity and omits boring default metadata.
    let session_id = result.output["session_id"].as_str().unwrap().to_string();
    assert!(session_id.starts_with("wc_sess_"));
    assert_eq!(result.output["project"], "demo");
    assert_eq!(result.output["resolved_project"], project);
    assert_eq!(result.output["continuation"], "created");
    for omitted in [
        "project_resolution",
        "execution_context",
        "repository",
        "jobs",
        "blockers",
        "suggested_next_actions",
        "deterministic",
        "llm_summary",
    ] {
        assert!(
            result.output.get(omitted).is_none(),
            "boring default field {omitted} should be omitted: {}",
            result.output
        );
    }
    // A failed advisory status probe remains non-blocking, while static model
    // guidance is now opt-in through context_request rather than primary output.
    assert!(result.output.get("readiness").is_none());
    assert!(result.output.get("warnings").is_none());
    assert_eq!(
        result.output["semantic_navigation"]["status"],
        "probe_failed"
    );
    assert_eq!(
        result.output["semantic_navigation"]["available"],
        Value::Null
    );
    assert!(result.output.get("workflow").is_none());
    assert!(result.output["instructions"].is_object());
    assert!(result.output["instructions"]
        .get("content_included")
        .is_none());
    assert!(result.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .all(|source| source.get("content").is_none()));
    for hidden in [
        "runtime_status",
        "connection_state",
        "authority",
        "read_tool_manifest",
        "recommended_flow",
        "startup_verdict",
        "git",
        "continuation_feedback",
    ] {
        assert!(
            !result.output.as_object().unwrap().contains_key(hidden),
            "compact output must not include {hidden}"
        );
    }

    // A new active normal session was created with the instruction as root.
    let summary = runtime.sessions.summary(&session_id, Some(50)).unwrap();
    assert_eq!(summary.project.as_deref(), Some(project.as_str()));
    assert_eq!(summary.mode, SessionMode::Normal);
    assert!(!summary.guards.deny_write_tools);
    assert!(!summary.guards.deny_shell_tools);
    let instructions = instruction_events(&runtime, &session_id);
    assert_eq!(instructions.len(), 1);
    assert_eq!(
        instructions[0].instruction.as_deref(),
        Some("first root instruction")
    );

    // A second call in the same window/project without session_id must create a
    // distinct Workflow Session instead of continuing the first implicitly.
    let second = dispatch_coding_call_in_window(
        &runtime,
        "wop-create",
        work_on_project_call("demo", "second root instruction", None),
        Some(&auth),
        "wop-create-window",
    )
    .await;
    assert!(second.success, "{:?}", second.error);
    let second_session_id = second.output["session_id"].as_str().unwrap();
    assert_ne!(second_session_id, session_id);
    assert_eq!(second.output["continuation"], "created");
    assert_eq!(
        runtime
            .sessions
            .active_session_count_for_test(Some(&project)),
        2
    );

    // Compact workspace/instruction projection reflects the underlying brief.
    assert!(result.output["workspace"]["branch"].is_string());
    assert!(result.output["instructions"]["status"].is_string());

    // The actual compact projection validates against its output schema.
    let schema = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
    let instance = json!({ "success": true, "output": result.output });
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&instance, &schema)
        .unwrap_or_else(|error| panic!("compact output must match its schema: {error}"));

    // B: an ordinary project tool stays unrecorded when neither a business
    // Session nor an explicit recording_session_id is supplied, even in the
    // same stable window that created the Workflow Sessions above.
    let first_before = runtime
        .sessions
        .summary(&session_id, Some(200))
        .unwrap()
        .events
        .len();
    let second_before = runtime
        .sessions
        .summary(second_session_id, Some(200))
        .unwrap()
        .events
        .len();
    let ordinary = call_hygiene_in_window_with_local_runner_transport(
        &runtime,
        "wop-create",
        &project,
        None,
        None,
        &auth,
        "ordinary_project_tool_without_explicit_session_is_unrecorded",
        ToolTransport::Api,
    )
    .await;
    assert!(ordinary.success, "{:?}", ordinary.error_status);
    assert_eq!(
        runtime
            .sessions
            .summary(&session_id, Some(200))
            .unwrap()
            .events
            .len(),
        first_before
    );
    assert_eq!(
        runtime
            .sessions
            .summary(second_session_id, Some(200))
            .unwrap()
            .events
            .len(),
        second_before
    );
}

#[tokio::test]
async fn same_window_recorder_gap_is_visible_without_backfilling_session_ledger() {
    let root = tempfile::tempdir().unwrap();
    let audit_root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let window_db = std::sync::Arc::new(
        crate::Database::open(&audit_root.path().join("window-activity.db")).unwrap(),
    );
    let runtime = ToolRuntime::new_for_tests().with_window_activity_database(window_db.clone());
    let project = register_runner_project_at_path(&runtime, "wop-gap", "demo", root.path()).await;
    let auth = auth_context(None, true);
    let window_id = "wop-recorder-gap-window";
    let window = crate::client_window::ClientWindow::for_test(window_id);

    // T1: the first work_on_project creates S and establishes the canonical
    // Window <-> Session relation even though no outer recorder existed yet.
    let created = dispatch_coding_call_in_window(
        &runtime,
        "wop-gap",
        work_on_project_call("demo", "create recorder-gap fixture", None),
        Some(&auth),
        window_id,
    )
    .await;
    assert!(created.success, "{:?}", created.error);
    let session_id = created.output["session_id"].as_str().unwrap().to_string();
    record_window_activity_fixture(
        &window_db,
        &auth,
        window_id,
        &project,
        "work_on_project",
        Some((
            &session_id,
            crate::action_audit_sessions::WorkflowSessionRelation::WorkOnProject,
        )),
        None,
        1_000,
    );

    // T2: an explicitly authorized outer recorder advances S normally.
    let recorded = call_hygiene_in_window_with_local_runner(
        &runtime,
        "wop-gap",
        &project,
        Some(&session_id),
        None,
        &auth,
        window_id,
    )
    .await;
    assert!(recorded.success, "{:?}", recorded.error_status);
    assert!(recorded.correlation.recorder_gap_session_id.is_none());
    assert!(recorded.correlation.business_session_id.is_none());
    assert!(recorded.correlation.workflow_sessions.iter().any(|link| {
        link.session_id == session_id
            && link.relation == crate::tool_runtime::WorkflowSessionCorrelationRelation::Recording
    }));
    record_window_activity_fixture(
        &window_db,
        &auth,
        window_id,
        &project,
        "check_workspace_hygiene",
        Some((
            &session_id,
            crate::action_audit_sessions::WorkflowSessionRelation::Recording,
        )),
        None,
        2_000,
    );
    let recorded_event_count = runtime
        .sessions
        .summary(&session_id, Some(200))
        .unwrap()
        .events
        .len();
    let guidance = runtime
        .sessions
        .post_message_with_ack(
            crate::tool_runtime::sessions::PostSessionMessageInput {
                session_id: session_id.clone(),
                kind: crate::tool_runtime::sessions::SessionMessageKind::Guidance,
                message: "same-window attention without recorder".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: crate::tool_runtime::sessions::SessionMessagePriority::High,
            },
            true,
        )
        .unwrap();

    // T3: omitting recording_session_id does not block business execution and
    // does not forge a Session event. The exact same Window/principal/Project
    // affinity produces a bounded recovery hint and a Window-only gap fact.
    let unrecorded = call_hygiene_in_window_with_local_runner(
        &runtime, "wop-gap", &project, None, None, &auth, window_id,
    )
    .await;
    assert!(unrecorded.success, "{:?}", unrecorded.error_status);
    assert!(unrecorded.correlation.business_session_id.is_none());
    assert_eq!(
        unrecorded.correlation.recorder_gap_session_id.as_deref(),
        Some(session_id.as_str())
    );
    let unrecorded_result = unrecorded.result.as_ref().expect("tool result");
    assert_eq!(
        unrecorded_result.output["workflow_recording_attention"]["status"],
        "recording_session_missing"
    );
    assert_eq!(
        unrecorded_result.output["workflow_recording_attention"]["candidate_session_id"],
        session_id
    );
    assert_eq!(
        unrecorded_result.output["workflow_recording_attention"]["project"],
        project
    );
    assert_eq!(
        unrecorded_result.output["session_attention"]["session_id"],
        session_id
    );
    assert_eq!(
        unrecorded_result.output["session_attention"]["source"],
        "window_affinity"
    );
    assert_eq!(
        unrecorded_result.output["session_attention"]["messages"][0]["message_id"],
        guidance.message_id
    );
    assert_eq!(
        unrecorded_result.output["session_attention"]["messages"][0]["message"],
        "same-window attention without recorder"
    );
    let affinity_ack_ref = unrecorded_result.output["session_attention"]["ack_ref"]
        .as_str()
        .expect("Window-affinity Session attention should expose ack_ref")
        .to_string();
    assert_eq!(
        runtime
            .sessions
            .summary(&session_id, Some(200))
            .unwrap()
            .events
            .len(),
        recorded_event_count,
        "missing recorder must not backfill or mutate the Workflow Session ledger"
    );

    let acknowledged = call_hygiene_in_window_with_local_runner_metadata(
        &runtime,
        "wop-gap",
        &project,
        None,
        None,
        &auth,
        window_id,
        ToolTransport::Mcp,
        ToolInvocationMetadata {
            ack_ref: Some(affinity_ack_ref),
            ..Default::default()
        },
    )
    .await;
    assert!(acknowledged.success, "{:?}", acknowledged.error_status);
    let acknowledged = acknowledged.result.unwrap();
    assert_eq!(
        acknowledged.output["session_attention"]["session_id"],
        session_id
    );
    assert_eq!(
        acknowledged.output["session_attention"]["ack"]["accepted_count"],
        1
    );
    assert!(acknowledged.output["session_attention"]["messages"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(runtime
        .sessions
        .list_messages(
            &session_id,
            crate::tool_runtime::sessions::ListSessionMessagesFilter {
                message_id: Some(guidance.message_id.clone()),
                ..Default::default()
            }
        )
        .unwrap()[0]
        .first_ack_observed_at
        .is_some());
    assert_eq!(
        runtime
            .sessions
            .summary(&session_id, Some(200))
            .unwrap()
            .events
            .len(),
        recorded_event_count,
        "fallback ACK must not create a recorder ToolCall event"
    );

    let forgotten = call_hygiene_in_window_with_local_runner(
        &runtime, "wop-gap", &project, None, None, &auth, window_id,
    )
    .await;
    assert_eq!(
        forgotten.result.unwrap().output["session_attention"]["messages"][0]["message_id"],
        guidance.message_id,
        "historical first ACK observation is not durable resolution"
    );

    // The recorder gap remains auditable, but an exact business Session equal to
    // the authorized same-Window candidate makes the model-facing reminder
    // redundant. The explicit business Session still receives its canonical
    // tool event; no recorder event is forged.
    let business_bound = call_hygiene_in_window_with_local_runner(
        &runtime,
        "wop-gap",
        &project,
        None,
        Some(&session_id),
        &auth,
        window_id,
    )
    .await;
    assert!(business_bound.success, "{:?}", business_bound.error_status);
    assert_eq!(
        business_bound
            .correlation
            .recorder_gap_session_id
            .as_deref(),
        Some(session_id.as_str())
    );
    assert_eq!(
        business_bound.correlation.business_session_id.as_deref(),
        Some(session_id.as_str())
    );
    assert!(business_bound
        .result
        .as_ref()
        .expect("business-bound result")
        .output
        .get("workflow_recording_attention")
        .is_none());
    assert!(
        runtime
            .sessions
            .summary(&session_id, Some(200))
            .unwrap()
            .events
            .len()
            > recorded_event_count,
        "explicit business Session must retain canonical business recording"
    );

    let different_session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("different business session".to_string()),
    );
    let different_business = call_hygiene_in_window_with_local_runner(
        &runtime,
        "wop-gap",
        &project,
        None,
        Some(&different_session.session_id),
        &auth,
        window_id,
    )
    .await;
    assert!(
        different_business.success,
        "{:?}",
        different_business.error_status
    );
    assert_eq!(
        different_business
            .correlation
            .recorder_gap_session_id
            .as_deref(),
        Some(session_id.as_str())
    );
    assert_eq!(
        different_business
            .correlation
            .business_session_id
            .as_deref(),
        Some(different_session.session_id.as_str())
    );
    assert_eq!(
        different_business
            .result
            .as_ref()
            .expect("different-business result")
            .output["workflow_recording_attention"]["candidate_session_id"],
        session_id
    );
    record_window_activity_fixture(
        &window_db,
        &auth,
        window_id,
        &project,
        "check_workspace_hygiene",
        None,
        unrecorded.correlation.recorder_gap_session_id.as_deref(),
        3_000,
    );

    let (principal_kind, principal_id) =
        crate::tool_runtime::runtime_observation_principal(Some(&auth)).unwrap();
    let window_rows = window_db
        .list_window_activity_events(window.key(), Some((&principal_kind, &principal_id)), 20)
        .unwrap();
    assert_eq!(
        window_rows
            .iter()
            .filter(|event| event.recorder_gap_session_id.as_deref() == Some(session_id.as_str()))
            .count(),
        1
    );

    // T4: explicitly echoing S restores normal recording. The original gap is
    // retained as history, but does not propagate to the recovered call.
    let recovered = call_hygiene_in_window_with_local_runner(
        &runtime,
        "wop-gap",
        &project,
        Some(&session_id),
        None,
        &auth,
        window_id,
    )
    .await;
    assert!(
        recovered.success,
        "transport={:?} result={:?}",
        recovered.error_status, recovered.result
    );
    assert!(recovered.correlation.recorder_gap_session_id.is_none());
    assert!(
        runtime
            .sessions
            .summary(&session_id, Some(200))
            .unwrap()
            .events
            .len()
            > recorded_event_count
    );
    record_window_activity_fixture(
        &window_db,
        &auth,
        window_id,
        &project,
        "check_workspace_hygiene",
        Some((
            &session_id,
            crate::action_audit_sessions::WorkflowSessionRelation::Recording,
        )),
        None,
        4_000,
    );
    let final_rows = window_db
        .list_window_activity_events(window.key(), Some((&principal_kind, &principal_id)), 20)
        .unwrap();
    assert_eq!(
        final_rows
            .iter()
            .filter(|event| event.recorder_gap_session_id.is_some())
            .count(),
        1,
        "recorder recovery must not propagate the prior gap"
    );
    let affinity = window_db
        .latest_window_workflow_affinity(window.key(), &principal_kind, &principal_id, &project)
        .unwrap()
        .unwrap();
    assert_eq!(affinity.workflow_session_id, session_id);
    assert_eq!(affinity.relation, "recording");
}

#[tokio::test]
async fn window_affinity_attention_is_strict_and_explicit_recorder_keeps_precedence() {
    let root = tempfile::tempdir().unwrap();
    let audit_root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let window_db = std::sync::Arc::new(
        crate::Database::open(&audit_root.path().join("attention-strict.db")).unwrap(),
    );
    let runtime = ToolRuntime::new_for_tests().with_window_activity_database(window_db.clone());
    let project =
        register_runner_project_at_path(&runtime, "attention-strict", "demo", root.path()).await;
    let auth = auth_context(None, true);
    let window_id = "attention-strict-window";
    let affinity = runtime
        .sessions
        .start_session(Some(project.clone()), Some("affinity target".to_string()));
    let affinity_message = runtime
        .sessions
        .post_message_with_ack(
            crate::tool_runtime::sessions::PostSessionMessageInput {
                session_id: affinity.session_id.clone(),
                kind: crate::tool_runtime::sessions::SessionMessageKind::Guidance,
                message: "affinity-only guidance".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: crate::tool_runtime::sessions::SessionMessagePriority::High,
            },
            true,
        )
        .unwrap();
    record_window_activity_fixture(
        &window_db,
        &auth,
        window_id,
        &project,
        "work_on_project",
        Some((
            &affinity.session_id,
            crate::action_audit_sessions::WorkflowSessionRelation::WorkOnProject,
        )),
        None,
        1_000,
    );

    let wrong_window = call_hygiene_in_window_with_local_runner(
        &runtime,
        "attention-strict",
        &project,
        None,
        None,
        &auth,
        "attention-strict-other-window",
    )
    .await;
    assert!(wrong_window.success);
    assert!(wrong_window
        .result
        .unwrap()
        .output
        .get("session_attention")
        .is_none());

    let other_principal = auth_context(Some("different-principal"), true);
    let wrong_principal = call_hygiene_in_window_with_local_runner(
        &runtime,
        "attention-strict",
        &project,
        None,
        None,
        &other_principal,
        window_id,
    )
    .await;
    assert!(wrong_principal.success);
    assert!(wrong_principal
        .result
        .unwrap()
        .output
        .get("session_attention")
        .is_none());

    let wrong_project_window = "attention-strict-wrong-project";
    let unrelated_project = "agent:other:unregistered-project";
    let unrelated = runtime.sessions.start_session(
        Some(unrelated_project.to_string()),
        Some("wrong project".to_string()),
    );
    record_window_activity_fixture(
        &window_db,
        &auth,
        wrong_project_window,
        unrelated_project,
        "work_on_project",
        Some((
            &unrelated.session_id,
            crate::action_audit_sessions::WorkflowSessionRelation::WorkOnProject,
        )),
        None,
        2_000,
    );
    let wrong_project = call_hygiene_in_window_with_local_runner(
        &runtime,
        "attention-strict",
        &project,
        None,
        None,
        &auth,
        wrong_project_window,
    )
    .await;
    assert!(wrong_project.success);
    assert!(wrong_project
        .result
        .unwrap()
        .output
        .get("session_attention")
        .is_none());

    let closed_window = "attention-strict-closed";
    let closed = runtime
        .sessions
        .start_session(Some(project.clone()), Some("closed affinity".to_string()));
    record_window_activity_fixture(
        &window_db,
        &auth,
        closed_window,
        &project,
        "work_on_project",
        Some((
            &closed.session_id,
            crate::action_audit_sessions::WorkflowSessionRelation::WorkOnProject,
        )),
        None,
        3_000,
    );
    runtime.sessions.close_session(&closed.session_id).unwrap();
    let inactive = call_hygiene_in_window_with_local_runner(
        &runtime,
        "attention-strict",
        &project,
        None,
        None,
        &auth,
        closed_window,
    )
    .await;
    assert!(inactive.success);
    assert!(inactive
        .result
        .unwrap()
        .output
        .get("session_attention")
        .is_none());

    let recorder = runtime
        .sessions
        .start_session(Some(project.clone()), Some("explicit recorder".to_string()));
    let recorder_message = runtime
        .sessions
        .post_message_with_ack(
            crate::tool_runtime::sessions::PostSessionMessageInput {
                session_id: recorder.session_id.clone(),
                kind: crate::tool_runtime::sessions::SessionMessageKind::Guidance,
                message: "explicit recorder guidance".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: crate::tool_runtime::sessions::SessionMessagePriority::Normal,
            },
            true,
        )
        .unwrap();
    let explicit = call_hygiene_in_window_with_local_runner(
        &runtime,
        "attention-strict",
        &project,
        Some(&recorder.session_id),
        None,
        &auth,
        window_id,
    )
    .await;
    assert!(explicit.success);
    let explicit_output = &explicit.result.as_ref().unwrap().output;
    assert_eq!(
        explicit_output["session_attention"]["session_id"],
        recorder.session_id
    );
    assert_eq!(
        explicit_output["session_attention"]["source"],
        "recording_session"
    );
    assert_eq!(
        explicit_output["session_attention"]["messages"][0]["message_id"],
        recorder_message.message_id
    );
    assert_ne!(
        explicit_output["session_attention"]["messages"][0]["message_id"],
        affinity_message.message_id
    );

    let resolution_without_recorder = call_hygiene_in_window_with_local_runner_metadata(
        &runtime,
        "attention-strict",
        &project,
        None,
        None,
        &auth,
        window_id,
        ToolTransport::Mcp,
        ToolInvocationMetadata {
            session_message_resolution: Some(
                crate::tool_runtime::sessions::ToolCallSessionMessageResolution {
                    message_id: affinity_message.message_id.clone(),
                    resolution: "must stay explicit".to_string(),
                },
            ),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        resolution_without_recorder.error_status,
        Some(
            crate::tool_runtime::kernel::ToolCallErrorStatus::InvalidArguments {
                message: "session_message_resolution requires recording_session_id".to_string(),
            }
        )
    );
    assert!(resolution_without_recorder.result.is_none());
    let retained = runtime
        .sessions
        .list_messages(
            &affinity.session_id,
            crate::tool_runtime::sessions::ListSessionMessagesFilter {
                message_id: Some(affinity_message.message_id),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        retained[0].status,
        crate::tool_runtime::sessions::SessionMessageStatus::Open
    );
    assert!(retained[0].resolution.is_none());
}

#[tokio::test]
async fn workflow_resume_context_is_window_principal_scoped_bounded_and_non_authoritative() {
    let root = tempfile::tempdir().unwrap();
    let audit_root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let window_db = std::sync::Arc::new(
        crate::Database::open(&audit_root.path().join("workflow-resume.db")).unwrap(),
    );
    let runtime = ToolRuntime::new_for_tests()
        .with_window_activity_database(window_db.clone())
        .with_project_reference_database(window_db.clone());
    let project =
        register_runner_project_at_path(&runtime, "workflow-resume", "demo", root.path()).await;
    let mut summary = named_registered_project(
        "workflow-resume",
        "demo",
        "demo",
        &root.path().to_string_lossy(),
        2,
    );
    summary.root_fingerprint = Some(format!("wc_projroot_{}", "a".repeat(64)));
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        "workflow-resume",
        "inst",
        vec![summary],
    )
    .await;
    let auth = auth_context(None, true);
    let owner_authority = crate::tool_runtime::workflow_session_authority_fingerprint(Some(&auth))
        .expect("test auth has stable Workflow Session authority");
    let start_owned_session = |project: Option<String>, title: Option<String>| {
        runtime
            .sessions
            .start_session_with_options(
                crate::tool_runtime::sessions::SessionCreateOptions::new(
                    project,
                    title,
                    SessionMode::Normal,
                    SessionGuards::default(),
                )
                .with_owner_authority_fingerprint(Some(owner_authority.clone())),
            )
            .unwrap()
    };
    let window_id = "workflow-resume-window";
    let window = crate::client_window::ClientWindow::for_test(window_id);

    assert_eq!(
        runtime
            .workflow_resume_context_projection_for_test(None, Some(&auth))
            .await
            .unwrap_err(),
        "client_window_unavailable"
    );

    let empty = runtime
        .workflow_resume_context_projection_for_test(Some(&window), Some(&auth))
        .await
        .unwrap();
    assert_eq!(empty["count"], 0);
    assert!(empty.get("suggested_call").is_none());

    let first = start_owned_session(
        Some(project.clone()),
        Some("implementation recovery candidate".to_string()),
    );
    record_window_activity_fixture(
        &window_db,
        &auth,
        window_id,
        &project,
        "work_on_project",
        Some((
            &first.session_id,
            crate::action_audit_sessions::WorkflowSessionRelation::WorkOnProject,
        )),
        None,
        1_000,
    );

    let single = runtime
        .workflow_resume_context_projection_for_test(Some(&window), Some(&auth))
        .await
        .unwrap();
    assert_eq!(single["count"], 1);
    assert_eq!(single["candidates"][0]["session_id"], first.session_id);
    let project_ref = single["candidates"][0]["project_ref"]
        .as_str()
        .expect("authorized Project with a root fingerprint should expose its short selector");
    assert!(project_ref.starts_with("~p"));
    assert_eq!(
        runtime
            .resolve_project_input_for_auth(project_ref, Some(&auth))
            .await
            .unwrap()
            .resolved_id,
        project
    );
    assert_eq!(single["selection"], "caller_must_choose_exact_session");
    let mut without_refs = runtime.clone();
    without_refs.project_reference_db = None;
    let canonical_only = without_refs
        .workflow_resume_context_projection_for_test(Some(&window), Some(&auth))
        .await
        .unwrap();
    assert_eq!(canonical_only["candidates"][0]["project"], project);
    assert!(canonical_only["candidates"][0].get("project_ref").is_none());
    let first_ref = single["candidates"][0]["session_ref"]
        .as_str()
        .expect("authorized discovery candidate should expose a short Session selector");
    assert!(first_ref.starts_with("~s"));
    assert_eq!(
        single["suggested_call"]["arguments"]["session_id"],
        first_ref
    );
    assert_eq!(
        single["suggested_call"]["follow_up_kind"],
        "fallback_recovery"
    );
    webcodex_tool_contracts::test_support::validate_generated_tool_call_against_registered_input_schema(
        &single["suggested_call"],
    )
    .expect("workflow resume recovery must pass read_session_handoff registered inputSchema");

    let other_window = crate::client_window::ClientWindow::for_test("workflow-resume-other");
    let hidden_by_window = runtime
        .workflow_resume_context_projection_for_test(Some(&other_window), Some(&auth))
        .await
        .unwrap();
    assert_eq!(hidden_by_window["count"], 0);

    let other_principal = auth_context(Some("different-user"), false);
    let hidden_by_principal = runtime
        .workflow_resume_context_projection_for_test(Some(&window), Some(&other_principal))
        .await
        .unwrap();
    assert_eq!(hidden_by_principal["count"], 0);

    let inaccessible_project = "agent:missing:workflow-resume";
    let inaccessible = start_owned_session(
        Some(inaccessible_project.to_string()),
        Some("must-not-leak-secret-title".to_string()),
    );
    record_window_activity_fixture(
        &window_db,
        &auth,
        window_id,
        inaccessible_project,
        "work_on_project",
        Some((
            &inaccessible.session_id,
            crate::action_audit_sessions::WorkflowSessionRelation::WorkOnProject,
        )),
        None,
        1_500,
    );
    let hidden_by_project_auth = runtime
        .workflow_resume_context_projection_for_test(Some(&window), Some(&auth))
        .await
        .unwrap();
    assert_eq!(hidden_by_project_auth["count"], 1);
    let hidden_json = serde_json::to_string(&hidden_by_project_auth).unwrap();
    assert!(!hidden_json.contains(&inaccessible.session_id));
    assert!(!hidden_json.contains("must-not-leak-secret-title"));

    let second = start_owned_session(
        Some(project.clone()),
        Some("independent review recovery candidate".to_string()),
    );
    record_window_activity_fixture(
        &window_db,
        &auth,
        window_id,
        &project,
        "work_on_project",
        Some((
            &second.session_id,
            crate::action_audit_sessions::WorkflowSessionRelation::WorkOnProject,
        )),
        None,
        2_000,
    );
    let multiple = runtime
        .workflow_resume_context_projection_for_test(Some(&window), Some(&auth))
        .await
        .unwrap();
    assert_eq!(multiple["count"], 2);
    assert_eq!(multiple["candidates"][0]["session_id"], second.session_id);
    assert_eq!(multiple["candidates"][1]["session_id"], first.session_id);
    assert!(multiple.get("suggested_call").is_none());
    runtime.sessions.close_session(&first.session_id).unwrap();
    let active_only = runtime
        .workflow_resume_context_projection_for_test(Some(&window), Some(&auth))
        .await
        .unwrap();
    assert_eq!(active_only["count"], 1);
    assert_eq!(
        active_only["candidates"][0]["session_id"],
        second.session_id
    );
    let second_ref = active_only["candidates"][0]["session_ref"]
        .as_str()
        .expect("remaining authorized candidate should keep its short selector");
    assert_eq!(
        active_only["suggested_call"]["arguments"]["session_id"],
        second_ref
    );
    assert_eq!(
        active_only["suggested_call"]["follow_up_kind"],
        "fallback_recovery"
    );

    let mut newest_session_id = String::new();
    for index in 0..8 {
        let extra = start_owned_session(
            Some(project.clone()),
            Some(format!("bounded recovery candidate {index}")),
        );
        newest_session_id = extra.session_id.clone();
        record_window_activity_fixture(
            &window_db,
            &auth,
            window_id,
            &project,
            "work_on_project",
            Some((
                &extra.session_id,
                crate::action_audit_sessions::WorkflowSessionRelation::WorkOnProject,
            )),
            None,
            3_000 + i64::from(index),
        );
    }
    let bounded = runtime
        .workflow_resume_context_projection_for_test(Some(&window), Some(&auth))
        .await
        .unwrap();
    assert_eq!(bounded["count"], 8);
    assert_eq!(bounded["truncated"], true);
    assert_eq!(
        bounded["candidates"][0]["session_id"], newest_session_id,
        "newest active candidate must sort first"
    );
    assert!(bounded.get("suggested_call").is_none());

    let scan_bound_window_id = "workflow-resume-scan-bound";
    let scan_bound_window = crate::client_window::ClientWindow::for_test(scan_bound_window_id);
    for index in 0..100 {
        let missing_session = format!("wc_sess_scan{index:012}");
        record_window_activity_fixture(
            &window_db,
            &auth,
            scan_bound_window_id,
            &project,
            "work_on_project",
            Some((
                &missing_session,
                crate::action_audit_sessions::WorkflowSessionRelation::WorkOnProject,
            )),
            None,
            10_000 + i64::from(index),
        );
    }
    let scan_bound = runtime
        .workflow_resume_context_projection_for_test(Some(&scan_bound_window), Some(&auth))
        .await
        .unwrap();
    assert_eq!(scan_bound["count"], 0);
    assert_eq!(
        scan_bound["truncated"], true,
        "saturating the bounded relation scan must not claim exhaustive recovery"
    );

    assert_eq!(
        runtime.sessions.lifecycle_state(&second.session_id),
        Some(crate::tool_runtime::sessions::SessionLifecycle::Active),
        "projection must not alter Session lifecycle"
    );
}

#[tokio::test]
async fn managed_worktree_bootstrap_recovers_same_operation_and_binds_session_to_final_project() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let managed = root.path().join("managed");
    let base_sha = seed_managed_tool_runtime_fixture(&source, &managed);
    let source_path = source.canonicalize().unwrap().to_string_lossy().to_string();
    let managed_path = managed
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let source_status_before = managed_fixture_git(&source, &["status", "--porcelain"]);
    let reference_db_dir = tempfile::tempdir().unwrap();
    let reference_db = std::sync::Arc::new(
        crate::Database::open(&reference_db_dir.path().join("project-refs.db")).unwrap(),
    );
    let runtime = ToolRuntime::new_for_tests().with_project_reference_database(reference_db);
    let client_id = "wop-managed";
    let mut source_project = registered_project("source", &source_path);
    source_project.root_fingerprint = Some(managed_source_root_fingerprint());
    register_agent_with_projects(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            project_path_registration: true,
            managed_worktree: true,
            internal_posix_script: true,
            ..Default::default()
        },
        vec![source_project],
    )
    .await;

    let auth = auth_context(None, true);
    let resolved_source = runtime
        .resolve_project_input_for_auth("agent:wop-managed:source", Some(&auth))
        .await
        .expect("source Project resolves for short-ref bootstrap");
    let source_project_ref = runtime
        .project_reference_for_resolved(&resolved_source, Some(&auth))
        .expect("source Project must have a short Project ref");
    assert!(source_project_ref.starts_with("~p"));

    let (first, payloads) = dispatch_with_managed_worktree_runner(
        &runtime,
        client_id,
        project_worktree_work_on_project_call(
            &source_project_ref,
            "work in an isolated checkout",
            Some("HEAD"),
            None,
        ),
        &source_path,
        &managed_path,
        "managed-a1b2c3d4",
        "HEAD",
        &base_sha,
        true,
        "managed_worktree_recovered",
        false,
        true,
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    assert_eq!(
        payloads.len(),
        2,
        "indeterminate response should re-observe once"
    );
    assert_eq!(payloads[0]["operation_id"], payloads[1]["operation_id"]);
    assert!(payloads[0]["resume_project_id"].is_null());
    assert_eq!(payloads[0]["base_ref"], "HEAD");
    assert_eq!(payloads[0]["expected_source_project_id"], "source");
    assert_eq!(
        payloads[0]["expected_source_root_fingerprint"],
        managed_source_root_fingerprint()
    );
    assert_eq!(
        payloads[1]["expected_source_project_id"],
        payloads[0]["expected_source_project_id"]
    );
    assert_eq!(
        payloads[1]["expected_source_root_fingerprint"],
        payloads[0]["expected_source_root_fingerprint"]
    );
    let project = "agent:wop-managed:managed-a1b2c3d4";
    assert_eq!(first.output["resolved_project"], project);
    assert_eq!(first.output["project"], project);
    let project_ref = first.output["project_ref"]
        .as_str()
        .expect("managed worktree bootstrap must return a short Project ref")
        .to_string();
    assert!(project_ref.starts_with("~p"));
    assert!(project_ref.len() < project.len());
    assert_eq!(first.output["worktree"]["managed"], true);
    assert_eq!(first.output["worktree"]["base_ref"], "HEAD");
    assert_eq!(first.output["worktree"]["base_sha"], base_sha);
    assert_eq!(first.output["worktree"]["source_dirty"], true);
    assert_eq!(
        first.output["knowledge_association"]["kind"],
        "managed_worktree_source"
    );
    assert_eq!(first.output["knowledge_association"]["status"], "available");
    assert_eq!(
        first.output["knowledge_association"]["source_project"],
        "agent:wop-managed:source"
    );
    assert_eq!(first.output["knowledge_association"]["base_sha"], base_sha);
    assert_eq!(first.output["knowledge_association"]["read_through"], false);
    assert_eq!(
        first.output["project_resolution"]["source"],
        "managed_worktree"
    );
    assert_eq!(
        first.output["project_resolution"]["outcome"],
        "managed_worktree_recovered"
    );
    assert_eq!(
        first.output["project_resolution"]["registered"], true,
        "lost-response recovery must preserve call-level registration mutation"
    );
    assert!(first.output["project_resolution"].get("worktree").is_none());
    let compact = first.output.to_string();
    assert!(!compact.contains(&source_path));
    assert!(!compact.contains(&managed_path));
    assert!(!compact.contains(&managed_source_root_fingerprint()));
    assert!(!compact.contains("managed_operation_id"));
    let session_id = first.output["session_id"].as_str().unwrap().to_string();
    let session = runtime.sessions.summary(&session_id, Some(50)).unwrap();
    assert_eq!(session.project.as_deref(), Some(project));

    let listed = runtime.list_projects(Some(&auth_context(None, true))).await;
    assert!(listed.output["projects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["id"] == project));

    let read = ToolCall::from_tool_name(
        "read_files",
        json!({"project": project_ref, "session_id": session_id, "items": [{"path": "hello.txt"}]}),
    )
    .unwrap();
    let read =
        dispatch_startup_without_window(&runtime, client_id, read, Some(&auth_context(None, true)))
            .await;
    assert!(read.success, "{:?}", read.error);
    assert!(read.output["items"][0]["output"]["text"]
        .as_str()
        .is_some_and(|text| text.contains("committed")));
    assert_eq!(
        std::fs::read_to_string(source.join("hello.txt")).unwrap(),
        "dirty source only\n"
    );
    assert_eq!(
        managed_fixture_git(&source, &["status", "--porcelain"]),
        source_status_before
    );

    let (resumed, resume_payloads) = dispatch_with_managed_worktree_runner(
        &runtime,
        client_id,
        worktree_work_on_project_call(
            client_id,
            &source_path,
            "continue the exact isolated checkout",
            None,
            Some(&session_id),
        ),
        &source_path,
        &managed_path,
        "managed-a1b2c3d4",
        "HEAD",
        &base_sha,
        true,
        "managed_worktree_recovered",
        false,
        false,
    )
    .await;
    assert!(resumed.success, "{:?}", resumed.error);
    assert_eq!(resumed.output["session_id"], session_id);
    assert_eq!(resumed.output["continuation"], "resumed_explicitly");
    assert_eq!(resumed.output["project_ref"], project_ref);
    assert_eq!(resume_payloads.len(), 1);
    assert_eq!(resume_payloads[0]["resume_project_id"], "managed-a1b2c3d4");
    assert!(resume_payloads[0]["base_ref"].is_null());
    assert_eq!(
        runtime
            .sessions
            .summary(&session_id, Some(50))
            .unwrap()
            .project
            .as_deref(),
        Some(project)
    );

    let schema = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
    let instance = json!({"success": true, "output": first.output});
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&instance, &schema)
        .unwrap_or_else(|error| panic!("managed worktree output must match schema: {error}"));
}

#[tokio::test]
async fn managed_worktree_project_source_failure_scrubs_internal_identity_fence() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    std::fs::create_dir_all(&source).unwrap();
    init_git_repo(&source);
    let source_path = source.canonicalize().unwrap().to_string_lossy().to_string();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "wop-managed-failure";
    let mut source_project = registered_project("source", &source_path);
    source_project.root_fingerprint = Some(managed_source_root_fingerprint());
    register_agent_with_projects(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            managed_worktree: true,
            ..Default::default()
        },
        vec![source_project],
    )
    .await;

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let call = project_worktree_work_on_project_call(
            "agent:wop-managed-failure:source",
            "surface a bounded identity failure",
            None,
            None,
        );
        async move {
            runtime
                .dispatch_with_auth(call, Some(&auth_context(None, true)))
                .await
        }
    });
    let deadline = std::time::Instant::now() + CODING_WORKFLOW_FIXTURE_TIMEOUT;
    while !task.is_finished() {
        assert!(
            std::time::Instant::now() < deadline,
            "managed-worktree failure fixture did not finish"
        );
        let Some(request) = probe_patch_agent_request(&runtime, client_id).await else {
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            continue;
        };
        assert_eq!(request.kind, "prepare_managed_worktree");
        let response = json!({
            "error_code": "managed_worktree_source_identity_changed",
            "error_kind": "managed_worktree_source_identity_changed",
            "failure_kind": "managed_worktree_source_identity_changed",
            "state_changed": false,
            "source_project_id": "source",
            "source_root_fingerprint": managed_source_root_fingerprint(),
        });
        complete_patch_agent_request(
            &runtime,
            client_id,
            &request.request_id,
            1,
            &response.to_string(),
            "",
        )
        .await;
    }

    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(
        result.output["error_kind"],
        "managed_worktree_source_identity_changed"
    );
    assert_eq!(
        result.output["source_project"],
        "agent:wop-managed-failure:source"
    );
    assert!(result.output.get("source_project_id").is_none());
    assert!(result.output.get("source_root_fingerprint").is_none());
}

#[tokio::test]
async fn managed_worktree_invalid_arguments_and_authority_fail_before_runner_mutation() {
    let runtime = ToolRuntime::new_for_tests();
    let root = tempfile::tempdir().unwrap();
    let source_path = root
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let invalid = runtime
        .dispatch(ToolCall::WorkOnProject {
            project: String::new(),
            client_id: Some("unresolved-runner".to_string()),
            path: Some(source_path.clone()),
            mode: Some("checkout".to_string()),
            base_ref: Some("main".to_string()),
            instruction: "must fail before resolution".to_string(),
            guidance_profile: Default::default(),
            include_extension_catalog: false,
            session_id: None,
        })
        .await;
    assert!(!invalid.success);
    assert_eq!(invalid.output["error_kind"], "invalid_arguments");
    assert_eq!(invalid.output["field"], "base_ref");
    assert_eq!(invalid.output["state_changed"], false);

    let client_id = "wop-managed-no-cap";
    register_agent_with_projects(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            project_path_registration: true,
            managed_worktree: false,
            ..Default::default()
        },
        Vec::new(),
    )
    .await;
    let unavailable = runtime
        .dispatch_with_auth(
            worktree_work_on_project_call(client_id, &source_path, "no mutation", None, None),
            Some(&auth_context(None, true)),
        )
        .await;
    assert!(!unavailable.success);
    assert_eq!(
        unavailable.output["error_kind"],
        "agent_capability_unavailable"
    );
    assert_eq!(unavailable.output["state_changed"], false);
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());

    let restricted = ToolRuntime::new_for_tests()
        .with_permission_evaluator(PermissionEvaluator::with_mode(AuthorityMode::Restricted));
    let restricted_client = "wop-managed-restricted";
    register_agent_with_projects(
        &restricted,
        restricted_client,
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            managed_worktree: true,
            ..Default::default()
        },
        Vec::new(),
    )
    .await;
    let denied = restricted
        .dispatch_with_auth(
            worktree_work_on_project_call(
                restricted_client,
                &source_path,
                "permission must precede prepare",
                None,
                None,
            ),
            Some(&auth_context(None, true)),
        )
        .await;
    assert!(!denied.success);
    assert_eq!(denied.output["error_kind"], "permission_denied");
    assert!(probe_patch_agent_request(&restricted, restricted_client)
        .await
        .is_none());
}

#[tokio::test]
async fn source_project_session_cannot_resume_a_managed_worktree() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    std::fs::create_dir_all(&source).unwrap();
    init_git_repo(&source);
    let source_path = source.canonicalize().unwrap().to_string_lossy().to_string();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "wop-managed-source-session";
    register_agent_with_projects(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            managed_worktree: true,
            ..Default::default()
        },
        vec![registered_project("source", &source_path)],
    )
    .await;
    let source_project = format!("agent:{client_id}:source");
    let auth = auth_context(None, true);
    let started = dispatch_coding_call_in_window(
        &runtime,
        client_id,
        work_on_project_call(&source_project, "source task", None),
        Some(&auth),
        "source-session-window",
    )
    .await;
    assert!(started.success, "{:?}", started.error);
    let session_id = started.output["session_id"].as_str().unwrap().to_string();

    let result = runtime
        .dispatch_with_auth(
            project_worktree_work_on_project_call(
                &source_project,
                "must not switch workspace",
                None,
                Some(&session_id),
            ),
            Some(&auth),
        )
        .await;
    assert!(
        probe_patch_agent_request(&runtime, client_id)
            .await
            .is_none(),
        "source Session mismatch must fail before prepare_managed_worktree"
    );
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "session_project_mismatch");
    assert_eq!(result.output["state_changed"], false);
    assert_eq!(instruction_events(&runtime, &session_id).len(), 1);
    assert_eq!(runtime.list_projects(Some(&auth)).await.output["count"], 1);
}

#[tokio::test]
async fn path_source_unknown_runner_returns_parser_ready_discovery_recovery() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let project_path = root.path().canonicalize().unwrap();
    let project_path = project_path.to_string_lossy().to_string();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "wop-missing-runner";
    let auth = auth_context(None, true);

    let outcome = runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: "work_on_project".to_string(),
                arguments: json!({
                    "client_id": client_id,
                    "path": project_path,
                    "instruction": "recover missing runner",
                }),
            },
            ToolCallContext {
                transport: ToolTransport::Mcp,
                session_id: None,
                auth: Some(&auth),
                window: None,
                record_oauth_scope_denials: true,
                host_file_import_trust: HostFileImportTrust::Untrusted,
            },
        )
        .await;
    assert!(
        outcome.error_status.is_none(),
        "unexpected transport error: {:?}",
        outcome.error_status
    );
    let result = outcome.result.expect("tool result");

    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "unknown_runner");
    assert_eq!(result.output["failure_kind"], "unknown_runner");
    assert_eq!(result.output["client_id"], client_id);
    assert_eq!(result.output["state_changed"], false);
    let suggested = &result.output["suggested_call"];
    assert_eq!(suggested["tool"], "list_runners");
    assert_eq!(
        suggested["arguments"],
        json!({"include_projects": false, "summary_only": true})
    );
    assert!(ToolCall::from_tool_name(
        suggested["tool"].as_str().unwrap(),
        suggested["arguments"].clone(),
    )
    .is_ok());
    let schema = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
    let instance = json!({"success": false, "output": result.output, "error": result.error});
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&instance, &schema)
        .unwrap_or_else(|error| panic!("unknown Runner recovery must match schema: {error}"));
}

#[tokio::test]
async fn path_source_auto_registers_reuses_and_supports_canonical_coding_entry() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    std::fs::write(root.path().join("hello.txt"), "hello\n").unwrap();
    let project_path = root.path().canonicalize().unwrap();
    let project_path = project_path.to_string_lossy().to_string();
    let references = tempfile::tempdir().unwrap();
    let db = std::sync::Arc::new(
        crate::Database::open(&references.path().join("references.db")).unwrap(),
    );
    let runtime = ToolRuntime::new_for_tests().with_project_reference_database(db);
    let client_id = "wop-path";
    register_agent_with_projects(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            project_path_registration: true,
            ..Default::default()
        },
        Vec::new(),
    )
    .await;

    let first = dispatch_with_path_runner(
        &runtime,
        client_id,
        path_work_on_project_call(client_id, &project_path, "first path instruction", None),
        "repo-a1b2c3d4",
        &project_path,
        "auto_registered",
        true,
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    assert_eq!(first.output["project_resolution"]["source"], "path");
    assert_eq!(
        first.output["project_resolution"]["outcome"],
        "auto_registered"
    );
    assert_eq!(first.output["project_resolution"]["registered"], true);
    assert_eq!(first.output["permission"]["status"], "auto_approved");
    assert_eq!(first.output["permission"]["tool_name"], "register_project");
    assert_eq!(
        first.output["resolved_project"],
        "agent:wop-path:repo-a1b2c3d4"
    );
    assert!(
        !first.output.to_string().contains(&project_path),
        "compact work_on_project output leaked the absolute input path"
    );
    assert_eq!(
        first.output["context_projection"]["materials"][0]["status"],
        "available"
    );
    assert_eq!(
        first.output["context_projection"]["materials"][0]["projection"]["fingerprint"],
        first.output["instructions"]["fingerprint"]
    );
    let session_id = first.output["session_id"].as_str().unwrap().to_string();

    let second = dispatch_with_path_runner(
        &runtime,
        client_id,
        path_work_on_project_call(
            client_id,
            &project_path,
            "second path instruction",
            Some(&session_id),
        ),
        "repo-a1b2c3d4",
        &project_path,
        "reused_existing_registration",
        false,
    )
    .await;
    assert!(second.success, "{:?}", second.error);
    assert_eq!(second.output["session_id"], session_id);
    assert_eq!(second.output["continuation"], "resumed_explicitly");
    assert_eq!(second.output["permission"]["status"], "auto_approved");
    assert_eq!(second.output["permission"]["tool_name"], "register_project");
    assert_eq!(
        second.output["project_resolution"]["outcome"],
        "reused_existing_registration"
    );
    assert_eq!(instruction_events(&runtime, &session_id).len(), 2);

    for selector in [
        first.output["resolved_project"].as_str().unwrap(),
        first.output["project_ref"].as_str().unwrap(),
    ] {
        let canonical = dispatch_with_path_runner(
            &runtime,
            client_id,
            work_on_project_call(selector, "canonical context", None),
            "repo-a1b2c3d4",
            &project_path,
            "reused_existing_registration",
            false,
        )
        .await;
        assert!(canonical.success, "{canonical:?}");
        assert_eq!(
            canonical.output["resolved_project"],
            first.output["resolved_project"]
        );
        assert_eq!(
            canonical.output["context_projection"]["materials"][0]["status"],
            "available"
        );
    }

    let listed = runtime.list_projects(Some(&auth_context(None, true))).await;
    assert!(listed.success);
    assert!(listed.output["projects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|project| project["id"] == "agent:wop-path:repo-a1b2c3d4"
            && project["source"] == "auto_registered"));

    let read = ToolCall::from_tool_name(
        "read_files",
        json!({
            "project": "agent:wop-path:repo-a1b2c3d4",
            "session_id": session_id,
            "items": [{"path": "hello.txt"}]
        }),
    )
    .unwrap();
    let read = dispatch_with_path_runner(
        &runtime,
        client_id,
        read,
        "repo-a1b2c3d4",
        &project_path,
        "reused_existing_registration",
        false,
    )
    .await;
    assert!(read.success, "{:?}", read.error);
    assert!(read.output["items"][0]["output"]["text"]
        .as_str()
        .is_some_and(|content| content.contains("hello")));
}

#[tokio::test]
async fn path_source_explicit_session_mismatch_fails_before_registration() {
    let first_root = tempfile::tempdir().unwrap();
    let second_root = tempfile::tempdir().unwrap();
    init_git_repo(first_root.path());
    init_git_repo(second_root.path());
    let first_path = first_root.path().canonicalize().unwrap();
    let second_path = second_root.path().canonicalize().unwrap();
    let first_path = first_path.to_string_lossy().to_string();
    let second_path = second_path.to_string_lossy().to_string();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "wop-path-mismatch";
    register_agent_with_projects(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            project_path_registration: true,
            ..Default::default()
        },
        Vec::new(),
    )
    .await;

    let first = dispatch_with_path_runner(
        &runtime,
        client_id,
        path_work_on_project_call(client_id, &first_path, "first project", None),
        "first-a1b2c3d4",
        &first_path,
        "auto_registered",
        true,
    )
    .await;
    assert!(first.success);
    let session_id = first.output["session_id"].as_str().unwrap();
    let mismatch = dispatch_with_path_runner(
        &runtime,
        client_id,
        path_work_on_project_call(
            client_id,
            &second_path,
            "must not fall back",
            Some(session_id),
        ),
        "second-a1b2c3d4",
        &second_path,
        "auto_registered",
        true,
    )
    .await;
    assert!(!mismatch.success);
    assert_eq!(mismatch.output["error_kind"], "session_project_mismatch");
    assert_eq!(mismatch.output["state_changed"], false);
    assert!(mismatch.output.get("permission").is_none());
    assert!(mismatch.output.get("project_resolution").is_none());
    assert_eq!(
        mismatch.output["request_project"],
        format!("path:{client_id}:{second_path}")
    );
    assert_eq!(instruction_events(&runtime, session_id).len(), 1);

    let listed = runtime.list_projects(Some(&auth_context(None, true))).await;
    assert_eq!(listed.output["count"], 1);
}

#[tokio::test]
async fn path_source_unknown_session_fails_before_registration() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let project_path = root.path().canonicalize().unwrap();
    let project_path = project_path.to_string_lossy().to_string();
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "wop-path-unknown";
    register_agent_with_projects(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            project_path_registration: true,
            ..Default::default()
        },
        Vec::new(),
    )
    .await;

    let result = dispatch_with_path_runner(
        &runtime,
        client_id,
        path_work_on_project_call(
            client_id,
            &project_path,
            "unknown must not fall back",
            Some("wc_sess_fedcba9876543210"),
        ),
        "unknown-a1b2c3d4",
        &project_path,
        "auto_registered",
        true,
    )
    .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "unknown_session_id");
    assert!(result.output.get("permission").is_none());
    assert!(result.output.get("project_resolution").is_none());
    let listed = runtime.list_projects(Some(&auth_context(None, true))).await;
    assert_eq!(listed.output["count"], 0);
}

#[tokio::test]
async fn path_source_cross_project_recording_session_fails_before_registration() {
    let recorder_root = tempfile::tempdir().unwrap();
    let target_root = tempfile::tempdir().unwrap();
    init_git_repo(recorder_root.path());
    init_git_repo(target_root.path());
    let target_path = target_root.path().canonicalize().unwrap();
    let target_path = target_path.to_string_lossy().to_string();
    let runtime = ToolRuntime::new_for_tests();
    let recorder_project = register_runner_project_at_path(
        &runtime,
        "wop-recorder-owner",
        "recorder",
        recorder_root.path(),
    )
    .await;
    let target_client = "wop-recorder-target";
    register_agent_with_projects(
        &runtime,
        target_client,
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            project_path_registration: true,
            ..Default::default()
        },
        Vec::new(),
    )
    .await;
    let auth = auth_context(None, true);
    let recorder = runtime.sessions.start_session(
        Some(recorder_project.clone()),
        Some("path recorder boundary".to_string()),
    );
    let recorder_session_id = recorder.session_id.clone();

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let target_path = target_path.clone();
        let recorder_session_id = recorder_session_id.clone();
        async move {
            runtime
                .call_tool_with_context(
                    crate::tool_runtime::kernel::ToolCallRequest {
                        tool_name: "work_on_project".to_string(),
                        arguments: json!({
                            "client_id": target_client,
                            "path": target_path,
                            "instruction": "bootstrap a different project through its path"
                        }),
                    },
                    crate::tool_runtime::kernel::ToolCallContext {
                        transport: crate::tool_runtime::kernel::ToolTransport::Api,
                        session_id: Some(&recorder_session_id),
                        auth: Some(&auth),
                        window: None,
                        record_oauth_scope_denials: true,
                        host_file_import_trust:
                            crate::tool_runtime::kernel::HostFileImportTrust::Untrusted,
                    },
                )
                .await
        }
    });

    let deadline = std::time::Instant::now() + CODING_WORKFLOW_FIXTURE_TIMEOUT;
    loop {
        if task.is_finished() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "kernel path bootstrap did not finish within {CODING_WORKFLOW_FIXTURE_TIMEOUT:?} for client {target_client}"
        );
        if let Some(request) = probe_patch_agent_request(&runtime, target_client).await {
            if request.kind == "resolve_or_register_project" {
                let payload: Value =
                    serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
                assert_eq!(payload["path"], target_path);
                let response = json!({
                    "id": "agent:wop-recorder-target:target-a1b2c3d4",
                    "agent_project_id": "target-a1b2c3d4",
                    "client_id": target_client,
                    "name": "target-a1b2c3d4",
                    "path": target_path,
                    "kind": "auto_registered",
                    "description": null,
                    "allow_patch": true,
                    "disabled": false,
                    "revision": format!("sha256:{}", "a".repeat(64)),
                    "source": "path",
                    "outcome": "auto_registered",
                    "registered": true,
                    "created_config": true,
                    "changed": true,
                    "recovered": false,
                });
                complete_patch_agent_request(
                    &runtime,
                    target_client,
                    &request.request_id,
                    0,
                    &response.to_string(),
                    "",
                )
                .await;
            } else {
                complete_agent_request_by_running_locally(&runtime, target_client, request).await;
            }
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
    let outcome = task.await.unwrap();
    assert!(!outcome.success);
    let result = outcome.result.expect("work_on_project mismatch result");
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "session_project_mismatch");
    assert_eq!(result.output["failure_kind"], "session_project_mismatch");
    assert_eq!(result.output["state_changed"], false);
    assert!(result.output.get("permission").is_none());
    assert_eq!(result.output["session_project"], recorder_project);
    assert_eq!(
        result.output["request_project"],
        format!("path:{target_client}:{target_path}")
    );
    let listed = runtime.list_projects(Some(&auth)).await;
    assert_eq!(listed.output["count"], 1);
    let summary = runtime
        .sessions
        .summary(&recorder_session_id, Some(50))
        .expect("recording session summary");
    let event = summary
        .events
        .iter()
        .rev()
        .find(|event| event.kind == "tool_call_finished" && event.tool_name == "work_on_project")
        .expect("recorded work_on_project event");
    assert_eq!(
        event.failure_kind.as_deref(),
        Some("session_project_mismatch")
    );
    assert_eq!(
        event.error_kind.as_deref(),
        Some("session_project_mismatch")
    );
    assert!(event.warning_kind.is_none());
    assert_eq!(
        event.session_project.as_deref(),
        Some(recorder_project.as_str())
    );
    assert_eq!(
        event.request_project.as_deref(),
        Some(format!("path:{target_client}:{target_path}").as_str())
    );
    assert!(event.permission.is_none());
}

#[tokio::test]
async fn path_source_requires_project_write_scope_before_runner_enqueue() {
    let root = tempfile::tempdir().unwrap();
    let project_path = root.path().canonicalize().unwrap();
    let project_path = project_path.to_string_lossy().to_string();
    let runtime = ToolRuntime::new_for_tests();
    let auth = managed_oauth_auth_context("path-read-only", Some("path-read-only-hash"));
    register_agent_projects_for_auth(
        &runtime,
        "oauth-client",
        &auth,
        RunnerCapabilities {
            shell: true,
            git: true,
            ..Default::default()
        },
        Vec::new(),
    )
    .await;

    let result = runtime
        .dispatch_with_auth(
            path_work_on_project_call("oauth-client", &project_path, "must not register", None),
            Some(&auth),
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "insufficient_scope");
    assert_eq!(
        result.output["required_scope"],
        crate::auth::SCOPE_PROJECT_WRITE
    );
    assert_eq!(result.output["state_changed"], false);
}

#[tokio::test]
async fn path_source_respects_restricted_authority_before_runner_enqueue() {
    let root = tempfile::tempdir().unwrap();
    let project_path = root.path().canonicalize().unwrap();
    let project_path = project_path.to_string_lossy().to_string();
    let runtime = ToolRuntime::new_for_tests()
        .with_permission_evaluator(PermissionEvaluator::with_mode(AuthorityMode::Restricted));
    let client_id = "wop-path-restricted";
    register_agent_with_projects(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            ..Default::default()
        },
        Vec::new(),
    )
    .await;

    let result = runtime
        .dispatch_with_auth(
            path_work_on_project_call(client_id, &project_path, "must not register", None),
            Some(&auth_context(None, true)),
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "permission_denied");
    assert_eq!(result.output["permission"]["status"], "denied");
    assert_eq!(result.output["permission"]["tool_name"], "register_project");
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
}

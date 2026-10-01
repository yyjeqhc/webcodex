
#[cfg(windows)]
#[tokio::test]
async fn windows_list_project_tracked_files_uses_internal_posix_and_preserves_scope() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    std::fs::create_dir_all(repo.path().join("src/nested")).unwrap();
    commit_file(repo.path(), "root.txt", "root\n", "root file");
    commit_file(repo.path(), "src/a.rs", "pub fn a() {}\n", "src file");
    commit_file(
        repo.path(),
        "src/nested/b.rs",
        "pub fn b() {}\n",
        "nested file",
    );

    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "tracked-windows", "demo", repo.path()).await;
    let (root, script) =
        run_windows_tracked_listing(&runtime, "tracked-windows", project.clone(), None).await;
    assert!(root.success, "{:?}", root.error);
    assert!(script.contains("git ls-files -z --cached"));
    assert!(
        script.contains("head_cmd")
            && script.contains(&format!("-c {}", LIST_TRACKED_SOURCE_MAX_BYTES + 1)),
        "tracked listing lost its raw-output cap: {script}"
    );
    let root_paths = root.output["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["path"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(root_paths, vec!["root.txt", "src/a.rs", "src/nested/b.rs"]);
    assert_eq!(root.output["source"], "git_index");
    assert_eq!(root.output["list_truncated"], false);

    let (scoped, _) = run_windows_tracked_listing(
        &runtime,
        "tracked-windows",
        project,
        Some("src".to_string()),
    )
    .await;
    assert!(scoped.success, "{:?}", scoped.error);
    assert_eq!(scoped.output["path"], "src");
    let scoped_paths = scoped.output["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["path"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(scoped_paths, vec!["src/a.rs", "src/nested/b.rs"]);
}

#[cfg(windows)]
#[tokio::test]
async fn windows_list_project_tracked_files_keeps_non_git_error_contract() {
    let project_dir = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "tracked-non-git", "demo", project_dir.path())
            .await;
    let (result, _) = run_windows_tracked_listing(&runtime, "tracked-non-git", project, None).await;
    assert!(!result.success);
    assert_eq!(result.output["code"], "not_a_git_repository");
}

#[cfg(windows)]
#[tokio::test]
async fn tracked_listing_failure_keeps_bounded_multiline_stderr() {
    let project_dir = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "tracked-diagnostic", "demo", project_dir.path())
            .await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .list_project_tracked_files(project, None, None, None, None, Some(100), Some(0))
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, "tracked-diagnostic").await;
    assert_eq!(request.kind, "run_internal_posix_script");
    let stderr = format!(
        "EARLY_DIAGNOSTIC_MUST_BE_TRUNCATED\n{}\nACTIONABLE_SECOND_LINE\nACTIONABLE_LAST_LINE",
        "x".repeat(LIST_TRACKED_STDERR_MAX_CHARS + 1024)
    );
    complete_patch_agent_request(
        &runtime,
        "tracked-diagnostic",
        &request.request_id,
        1,
        "",
        &stderr,
    )
    .await;
    let result = task.await.unwrap();
    assert!(!result.success);
    let error = result.error.as_deref().unwrap_or_default();
    assert!(error.contains("ACTIONABLE_SECOND_LINE"), "{error}");
    assert!(error.contains("ACTIONABLE_LAST_LINE"), "{error}");
    assert!(
        error.contains('\n'),
        "stderr excerpt must remain multi-line: {error}"
    );
    assert!(!error.contains("EARLY_DIAGNOSTIC_MUST_BE_TRUNCATED"));
    assert!(
        error.chars().count() <= LIST_TRACKED_STDERR_MAX_CHARS + 64,
        "bounded stderr grew unexpectedly: {} chars",
        error.chars().count()
    );
}

async fn run_mocked_tracked_listing(
    client_id: &str,
    stdout: String,
    depth: Option<usize>,
    limit: usize,
    offset: usize,
) -> (ToolResult, String) {
    let runtime = runtime_with_agent_project(client_id);
    register_agent(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            shell: true,
            internal_posix_script: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client_id);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .list_project_tracked_files(
                    project,
                    None,
                    None,
                    None,
                    depth,
                    Some(limit),
                    Some(offset),
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    assert_eq!(request.kind, "run_internal_posix_script");
    let script = request
        .script
        .as_ref()
        .expect("tracked listing must use the typed internal POSIX path")
        .script
        .clone();
    complete_patch_agent_request(&runtime, client_id, &request.request_id, 0, &stdout, "").await;
    (task.await.unwrap(), script)
}

#[tokio::test]
async fn tracked_listing_source_budget_is_below_default_retention_and_disables_fake_paging() {
    const ORDINARY_RESULT_RETENTION_COMPATIBILITY_FLOOR_BYTES: usize = 256 * 1024;
    assert!(
        LIST_TRACKED_SOURCE_MAX_BYTES + 1 < ORDINARY_RESULT_RETENTION_COMPATIBILITY_FLOOR_BYTES,
        "producer source probe must keep headroom below ordinary per-stream result retention"
    );

    let mut raw = String::new();
    let mut index = 0usize;
    while raw.len() <= LIST_TRACKED_SOURCE_MAX_BYTES + 4096 {
        raw.push_str(&format!("src/file-{index:05}-{}.rs\0", "x".repeat(20)));
        index += 1;
    }
    assert!(raw.len() < ORDINARY_RESULT_RETENTION_COMPATIBILITY_FLOOR_BYTES);

    let (result, script) =
        run_mocked_tracked_listing("tracked-source-budget", raw, Some(16), 10, 0).await;
    assert!(result.success, "{:?}", result.error);
    assert!(script.contains(&format!("-c {}", LIST_TRACKED_SOURCE_MAX_BYTES + 1)));
    assert_eq!(result.output["list_truncated"], true);
    assert_eq!(result.output["truncated"], false);
    assert_eq!(result.output["next_offset"], Value::Null);
    assert_eq!(result.output["returned"], 10);
    assert!(result.output["total_files"].as_u64().unwrap() >= 10);
}

#[tokio::test]
async fn tracked_listing_fails_closed_when_server_retains_only_stdout_tail() {
    let mut raw = String::new();
    let mut index = 0usize;
    while raw.len() <= 300 * 1024 {
        raw.push_str(&format!("src/file-{index:05}-{}.rs\0", "y".repeat(32)));
        index += 1;
    }

    let (result, _) =
        run_mocked_tracked_listing("tracked-retained-tail", raw, Some(16), 10, 0).await;
    assert!(!result.success);
    assert_eq!(result.output["code"], "source_incomplete");
    assert!(result.output.get("next_offset").is_none());
    assert!(result.output.get("total_files").is_none());
    assert!(result
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("ordinary result retention"));
}

#[tokio::test]
async fn write_project_file_with_session_id_records_changed_path_without_content() {
    let runtime = runtime_with_agent_project("telemetry-write");
    let caps = RunnerCapabilities {
        file_write: true,
        shell: true,
        git: true,
        internal_posix_script: true,
        ..Default::default()
    };
    register_agent(&runtime, "telemetry-write", None, caps).await;
    let project = agent_test_project_id("telemetry-write");
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::WriteProjectFile {
                        project,
                        path: "src/new.txt".to_string(),
                        content: "do-not-log-this-content\n".to_string(),
                        session_id: Some(session_id),
                        overwrite: None,
                        expected_read_revision: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = wait_for_patch_agent_request(&runtime, "telemetry-write").await;
    assert_eq!(req.kind, "file_write_project_file");
    assert!(req.command.is_empty());
    assert!(req.stdin.is_none());
    let payload: serde_json::Value =
        serde_json::from_str(req.content.as_deref().expect("file-op payload")).unwrap();
    assert_eq!(payload["path"], "src/new.txt");
    assert_eq!(payload["content"], "do-not-log-this-content\n");
    complete_patch_agent_request(
        &runtime,
        "telemetry-write",
        &req.request_id,
        0,
        r#"{"path":"src/new.txt","bytes_written":24,"sha256":"abc","changed":true,"state_changed":true,"execution_state":"completed"}"#,
        "",
    )
    .await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["permission"]["required"], true);
    assert_eq!(result.output["state_changed"], true);
    assert_eq!(result.output["permission"]["policy"], "trusted_agent");
    assert_eq!(result.output["permission"]["status"], "auto_approved");
    assert_eq!(
        result.output["permission"]["reason"],
        "trusted_agent_authority"
    );
    assert_eq!(result.output["permission"]["risk"], "write");
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.write_like, 1);
    let event = finished_event(&summary, "write_project_file");
    assert!(event.write_like);
    assert_eq!(event.changed_paths, vec!["src/new.txt".to_string()]);
    assert_eq!(
        event
            .effect_evidence
            .as_ref()
            .and_then(|evidence| evidence.state_changed),
        Some(true)
    );
    let permission = event.permission.as_ref().expect("permission metadata");
    assert!(permission.required);
    assert_eq!(permission.policy, "trusted_agent");
    assert_eq!(permission.status, "auto_approved");
    assert_eq!(permission.tool_name, "write_project_file");
    assert_eq!(permission.risk, "write");
    let serialized = serde_json::to_string(&summary.events).unwrap();
    assert!(
        !serialized.contains("do-not-log-this-content"),
        "session event leaked write content: {serialized}"
    );

    let handoff = runtime
        .dispatch(ToolCall::SessionHandoffSummary {
            session_id: session.session_id.clone(),
            project: None,
            include_workspace: Some(false),
            include_checkpoints: Some(false),
            include_validation: Some(false),
            diagnostic: true,
            limit: None,
        })
        .await;
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(handoff.output["permissions"]["required_count"], 1);
    assert_eq!(handoff.output["permissions"]["auto_approved_count"], 1);
    assert_eq!(handoff.output["permissions"]["manual_approved_count"], 0);
    assert_eq!(handoff.output["permissions"]["total_approved_count"], 1);
    assert_eq!(
        handoff.output["permissions"]["recent"][0]["tool_name"],
        "write_project_file"
    );

    let finish_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::FinishCodingTask {
                        outputs: Vec::new(),
                        project,
                        session_id,
                        summary_only: false,
                        include_diff: Some(false),
                        include_workspace: None,
                        include_hygiene: Some(false),
                        include_handoff: Some(false),
                        include_validation_summary: Some(false),
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = wait_for_patch_agent_request(&runtime, "telemetry-write").await;
    assert_internal_posix_script_contains(&req, "git status --porcelain=v1 -b");
    let show_changes_stdout =
        crate::tool_runtime::framed_clean_show_changes_test_stdout("write", false);
    complete_patch_agent_request(
        &runtime,
        "telemetry-write",
        &req.request_id,
        0,
        &show_changes_stdout,
        "",
    )
    .await;
    let finish = finish_task.await.unwrap();
    assert!(finish.success, "{:?}", finish.error);
    assert_eq!(finish.output["permissions"]["required_count"], 1);
    assert_eq!(finish.output["permissions"]["auto_approved_count"], 1);
    assert_eq!(finish.output["permissions"]["manual_approved_count"], 0);
    assert_eq!(finish.output["permissions"]["total_approved_count"], 1);
}

#[tokio::test]
async fn delete_project_files_capable_agent_uses_structured_delete_without_output_leaks() {
    let runtime = runtime_with_agent_project("cleanup-delete");
    register_agent(
        &runtime,
        "cleanup-delete",
        None,
        RunnerCapabilities {
            structured_file_delete: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("cleanup-delete");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .delete_project_files(project, vec!["tmp.txt".to_string()])
                .await
        }
    });

    let req = wait_for_patch_agent_request(&runtime, "cleanup-delete").await;
    assert_eq!(req.kind, "file_delete_project_files");
    assert!(req.command.is_empty());
    assert_eq!(req.path.as_deref(), Some("."));
    let payload: Value = serde_json::from_str(req.content.as_deref().unwrap()).unwrap();
    assert_eq!(payload, json!({"paths": ["tmp.txt"]}));
    complete_patch_agent_request(
        &runtime,
        "cleanup-delete",
        &req.request_id,
        0,
        r#"{"deleted_paths":["tmp.txt"]}"#,
        "/private/runner/path raw stderr must not leak\n",
    )
    .await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["ok"], true);
    assert_eq!(result.output["state_changed"], true);
    assert_eq!(result.output["deleted_paths"], json!(["tmp.txt"]));
    assert_eq!(result.output["stdout_present"], false);
    assert_eq!(result.output["stderr_present"], false);
    let serialized = serde_json::to_string(&result.output).unwrap();
    assert!(!serialized.contains("/private/runner/path"));
    assert!(!serialized.contains("raw stderr"));
}

/// Wait until the client has at least `expected` pending requests without
/// polling/dispatching them. The single wall-clock deadline is deliberately
/// independent of scheduler yield counts.
async fn wait_for_pending_requests(runtime: &ToolRuntime, client_id: &str, expected: usize) {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let view = runtime
            .runner_registry
            .get_runner_view(client_id)
            .await
            .unwrap_or_else(|| panic!("client {client_id} must be registered"));
        let pending = view.pending_requests;
        if pending >= expected {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "pending requests did not reach {expected} within 10 seconds for {client_id}; last pending count={pending}"
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}

#[tokio::test]
async fn delete_project_files_replacement_before_poll_reports_not_started() {
    let runtime = runtime_with_agent_project("cleanup-delete-replace-early");
    register_agent(
        &runtime,
        "cleanup-delete-replace-early",
        None,
        RunnerCapabilities {
            structured_file_delete: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("cleanup-delete-replace-early");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .delete_project_files(project, vec!["tmp.txt".to_string()])
                .await
        }
    });

    // Wait until the structured request is queued, without polling it.
    wait_for_pending_requests(&runtime, "cleanup-delete-replace-early", 1).await;
    // Replace the Runner process before the request was ever polled.
    runtime
        .runner_registry
        .set_last_seen_for_test(
            "cleanup-delete-replace-early",
            chrono::Utc::now().timestamp() - 120,
        )
        .await;
    register_agent_with_instance(
        &runtime,
        "cleanup-delete-replace-early",
        "inst-b",
        None,
        RunnerCapabilities::default(),
    )
    .await;

    let result = task.await.unwrap();
    assert!(!result.success, "{:?}", result.error);
    assert_eq!(result.output["execution_state"], "not_started");
    assert_eq!(result.output["failure_kind"], "not_started");
    assert_eq!(result.output["tool_failure"], true);
    assert_ne!(result.output["execution_state"], "outcome_unknown");
    let error = result.error.as_deref().unwrap_or_default();
    assert!(
        error.contains("was not dispatched") && error.contains("did not start"),
        "error was: {error}"
    );

    // No legacy fallback and no inherited structured request for the
    // replacement Runner.
    let extra =
        probe_agent_request_for_instance(&runtime, "cleanup-delete-replace-early", "inst-b").await;
    assert!(
        extra.is_none(),
        "replacement Runner must receive no request: {extra:?}"
    );
}

#[tokio::test]
async fn delete_project_files_replacement_after_poll_reports_outcome_unknown() {
    let runtime = runtime_with_agent_project("cleanup-delete-replace-late");
    register_agent(
        &runtime,
        "cleanup-delete-replace-late",
        None,
        RunnerCapabilities {
            structured_file_delete: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("cleanup-delete-replace-late");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .delete_project_files(project, vec!["tmp.txt".to_string()])
                .await
        }
    });

    wait_for_pending_requests(&runtime, "cleanup-delete-replace-late", 1).await;
    // Dispatch the structured request to the original instance.
    let req =
        wait_for_runner_request_for_instance(&runtime, "cleanup-delete-replace-late", "inst").await;
    assert_eq!(req.kind, "file_delete_project_files");
    // Replace the Runner before it returns its result.
    runtime
        .runner_registry
        .set_last_seen_for_test(
            "cleanup-delete-replace-late",
            chrono::Utc::now().timestamp() - 120,
        )
        .await;
    register_agent_with_instance(
        &runtime,
        "cleanup-delete-replace-late",
        "inst-b",
        None,
        RunnerCapabilities::default(),
    )
    .await;

    let result = task.await.unwrap();
    assert!(!result.success, "{:?}", result.error);
    assert_eq!(result.output["execution_state"], "outcome_unknown");
    assert_eq!(result.output["failure_kind"], "outcome_unknown");
    assert_eq!(result.output["tool_failure"], true);
    let error = result.error.as_deref().unwrap_or_default();
    assert!(
        error.contains("may already have deleted files"),
        "error was: {error}"
    );
    assert!(
        error.contains("Inspect current workspace state"),
        "error was: {error}"
    );

    // The replacement Runner cannot complete or inherit the dispatched
    // request, and no legacy fallback is emitted.
    let err = runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: "cleanup-delete-replace-late".to_string(),
            runner_instance_id: "inst-b".to_string(),
            request_id: req.request_id,
            exit_code: Some(0),
            stdout: Some(r#"{"deleted_paths":["tmp.txt"]}"#.to_string()),
            stderr: None,
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap_err();
    assert!(
        err.contains("unknown or expired shell request"),
        "replacement must not complete the replaced request: {err}"
    );
    let extra =
        probe_agent_request_for_instance(&runtime, "cleanup-delete-replace-late", "inst-b").await;
    assert!(
        extra.is_none(),
        "replacement Runner must receive no inherited request: {extra:?}"
    );
}

#[tokio::test]
async fn delete_project_files_timeout_before_dispatch_reports_not_started() {
    let runtime = runtime_with_agent_project("cleanup-delete-timeout-early");
    register_agent(
        &runtime,
        "cleanup-delete-timeout-early",
        None,
        RunnerCapabilities {
            structured_file_delete: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("cleanup-delete-timeout-early");
    let proj = runtime.resolve_project(&project).await.unwrap();
    let client_id = proj.client_id.clone();

    // Short wait bound: the request is never polled, so the wait timeout fires
    // and the dispatch-aware cancellation proves the request was never
    // dispatched.
    let result = runtime
        .delete_project_files_structured_agent(&proj, client_id, vec!["tmp.txt".to_string()], 1)
        .await;

    assert!(!result.success, "{:?}", result.error);
    assert_eq!(result.output["execution_state"], "not_started");
    assert_eq!(result.output["failure_kind"], "not_started");
    assert_eq!(result.output["tool_failure"], true);
    let error = result.error.as_deref().unwrap_or_default();
    assert!(
        error.contains("timed out") && error.contains("did not start"),
        "error was: {error}"
    );
    // The timed-out request was removed: no queue/waiter leak.
    let view = runtime
        .runner_registry
        .get_runner_view("cleanup-delete-timeout-early")
        .await
        .unwrap();
    assert_eq!(view.pending_requests, 0);
}

#[tokio::test]
async fn delete_project_files_timeout_after_dispatch_reports_outcome_unknown() {
    let runtime = runtime_with_agent_project("cleanup-delete-timeout-late");
    register_agent(
        &runtime,
        "cleanup-delete-timeout-late",
        None,
        RunnerCapabilities {
            structured_file_delete: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("cleanup-delete-timeout-late");
    let proj = runtime.resolve_project(&project).await.unwrap();
    let client_id = proj.client_id.clone();
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let client_id = client_id.clone();
        async move {
            runtime
                .delete_project_files_structured_agent(
                    &proj,
                    client_id,
                    vec!["tmp.txt".to_string()],
                    1,
                )
                .await
        }
    });

    wait_for_pending_requests(&runtime, "cleanup-delete-timeout-late", 1).await;
    // Dispatch the structured request; the Runner never returns a result, so
    // the wait timeout fires after dispatch may have started deleting.
    let req =
        wait_for_runner_request_for_instance(&runtime, "cleanup-delete-timeout-late", "inst").await;
    assert_eq!(req.kind, "file_delete_project_files");

    let result = task.await.unwrap();
    assert!(!result.success, "{:?}", result.error);
    assert_eq!(result.output["execution_state"], "outcome_unknown");
    assert_eq!(result.output["failure_kind"], "outcome_unknown");
    let error = result.error.as_deref().unwrap_or_default();
    assert!(
        error.contains("timed out") && error.contains("may already have deleted files"),
        "error was: {error}"
    );
    // The timed-out request was removed: no queue/waiter leak.
    let view = runtime
        .runner_registry
        .get_runner_view("cleanup-delete-timeout-late")
        .await
        .unwrap();
    assert_eq!(view.pending_requests, 0);
}

#[tokio::test]
async fn delete_project_files_waiter_dropped_without_undispatch_proof_reports_outcome_unknown() {
    let runtime = runtime_with_agent_project("cleanup-delete-waiter");
    register_agent(
        &runtime,
        "cleanup-delete-waiter",
        None,
        RunnerCapabilities {
            structured_file_delete: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("cleanup-delete-waiter");
    let proj = runtime.resolve_project(&project).await.unwrap();
    let client_id = proj.client_id.clone();
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let client_id = client_id.clone();
        async move {
            runtime
                .delete_project_files_structured_agent(
                    &proj,
                    client_id,
                    vec!["tmp.txt".to_string()],
                    30,
                )
                .await
        }
    });

    wait_for_pending_requests(&runtime, "cleanup-delete-waiter", 1).await;
    // Manufacture the dropped waiter through the existing dispatch-state-aware
    // cancellation API: remove the pending record (dropping the oneshot
    // sender) without resolving it, so the tool's receiver observes the
    // channel close. The registry returns the preserved dispatch truth.
    let req = wait_for_runner_request_for_instance(&runtime, "cleanup-delete-waiter", "inst").await;
    let dispatch = runtime
        .runner_registry
        .cancel_request_dispatch_state(&req.request_id)
        .await;
    assert_eq!(
        dispatch,
        Some(true),
        "registry must preserve dispatch truth for the cancelled request"
    );

    // The subsequent cancellation in the tool finds no record, which cannot
    // prove undispatch: the result must be outcome_unknown, never not_started.
    let result = task.await.unwrap();
    assert!(!result.success, "{:?}", result.error);
    assert_eq!(result.output["execution_state"], "outcome_unknown");
    assert_eq!(result.output["failure_kind"], "outcome_unknown");
    assert_eq!(result.output["tool_failure"], true);
    let error = result.error.as_deref().unwrap_or_default();
    assert!(
        error.contains("may already have deleted files"),
        "error was: {error}"
    );
    let view = runtime
        .runner_registry
        .get_runner_view("cleanup-delete-waiter")
        .await
        .unwrap();
    assert_eq!(view.pending_requests, 0);
}

#[tokio::test]
async fn delete_project_files_terminal_failure_reports_outcome_unknown() {
    let runtime = runtime_with_agent_project("cleanup-delete-terminal");
    register_agent(
        &runtime,
        "cleanup-delete-terminal",
        None,
        RunnerCapabilities {
            structured_file_delete: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("cleanup-delete-terminal");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .delete_project_files(project, vec!["tmp.txt".to_string()])
                .await
        }
    });

    wait_for_pending_requests(&runtime, "cleanup-delete-terminal", 1).await;
    // The Runner returns a definitive terminal failure after dispatch
    // (non-zero exit). The mutation may already have deleted files, so the
    // failure must never collapse into an ordinary retry-safe error.
    let req =
        wait_for_runner_request_for_instance(&runtime, "cleanup-delete-terminal", "inst").await;
    complete_patch_agent_request_for_instance(
        &runtime,
        "cleanup-delete-terminal",
        "inst",
        &req.request_id,
        1,
        "",
        "delete failed",
    )
    .await;

    let result = task.await.unwrap();
    assert!(!result.success, "{:?}", result.error);
    assert_eq!(result.output["execution_state"], "outcome_unknown");
    assert_eq!(result.output["failure_kind"], "outcome_unknown");
    assert_eq!(result.output["tool_failure"], true);
    let error = result.error.as_deref().unwrap_or_default();
    assert!(
        error.contains("may already have deleted files"),
        "error was: {error}"
    );
    // No automatic legacy fallback follows the uncertain mutation.
    let extra = probe_agent_request_for_instance(&runtime, "cleanup-delete-terminal", "inst").await;
    assert!(
        extra.is_none(),
        "no legacy fallback may follow an uncertain structured delete: {extra:?}"
    );
}

#[tokio::test]
async fn artifact_upload_chunk_session_log_arguments_do_not_store_base64() {
    let runtime = runtime_with_agent_project("telemetry-artifact-chunk");
    register_agent(
        &runtime,
        "telemetry-artifact-chunk",
        None,
        RunnerCapabilities {
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("telemetry-artifact-chunk");
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let raw_marker = "SECRET_CHUNK_CONTENT_SHOULD_NOT_BE_LOGGED";
    let content_base64 =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, raw_marker);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        let content_base64 = content_base64.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::ArtifactUploadChunk {
                        project,
                        path: "artifacts/imports/chunk.txt".to_string(),
                        upload_id: "wc_upload_test_1".to_string(),
                        offset: 7,
                        content_base64,
                        session_id: Some(session_id),
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });

    let req = wait_for_patch_agent_request(&runtime, "telemetry-artifact-chunk").await;
    let payload: serde_json::Value =
        serde_json::from_str(req.content.as_deref().expect("file-op payload")).unwrap();
    assert_eq!(payload["content_base64"], content_base64);
    complete_patch_agent_request(
        &runtime,
        "telemetry-artifact-chunk",
        &req.request_id,
        0,
        r#"{"path":"artifacts/imports/chunk.txt","upload_id":"wc_upload_test_1","received_bytes":12,"next_offset":12,"expected_bytes":null,"expected_sha256":null,"max_bytes":268435456,"mime_type":null,"committed":false}"#,
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    let started = summary
        .events
        .iter()
        .rev()
        .find(|event| {
            event.kind == "tool_call_started" && event.tool_name == "upload_artifact_chunk"
        })
        .expect("started event for upload_artifact_chunk");
    let input_summary = started
        .input_summary
        .as_ref()
        .expect("input_summary present on started event");
    assert_eq!(input_summary["path"], "artifacts/imports/chunk.txt");
    assert_eq!(input_summary["upload_id"], "wc_upload_test_1");
    assert_eq!(input_summary["offset"], 7);
    assert_eq!(input_summary["content_base64_present"], true);
    assert!(input_summary.get("content_base64").is_none());
    let serialized = serde_json::to_string(&summary.events).unwrap();
    assert!(
        !serialized.contains(&content_base64) && !serialized.contains(raw_marker),
        "session event leaked base64 chunk content: {serialized}"
    );
}

#[test]
fn conversation_import_session_log_arguments_do_not_store_host_file_refs() {
    let download_url = "https://files.oaiusercontent.com/NEVER_PERSIST_IMPORT_URL";
    let file_id = "NEVER_PERSIST_IMPORT_FILE_ID";
    let arguments = serde_json::json!({
        "project": "agent:test:demo",
        "openaiFileIdRefs": [{
            "download_url": download_url,
            "file_id": file_id,
            "mime_type": "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            "file_name": "private-name.pptx"
        }],
        "output_dir": "paper/export",
        "targets": ["import-test.pptx"],
        "overwrite": false
    });

    let raw_summary = super::super::tool_audit::session_log_arguments_for_tool_request(
        "import_host_files",
        &arguments,
    );
    assert_eq!(raw_summary["project"], "agent:test:demo");
    assert_eq!(raw_summary["file_count"], 1);
    assert_eq!(raw_summary["targets_count"], 1);
    let raw_json = serde_json::to_string(&raw_summary).unwrap();
    assert!(!raw_json.contains(download_url));
    assert!(!raw_json.contains(file_id));
    assert!(!raw_json.contains("private-name.pptx"));

    let call = ToolCall::from_tool_name("import_host_files", arguments).unwrap();
    let typed_summary = call.session_log_arguments();
    assert_eq!(typed_summary["project"], "agent:test:demo");
    assert_eq!(typed_summary["file_count"], 1);
    assert_eq!(typed_summary["targets_count"], 1);
    let typed_json = serde_json::to_string(&typed_summary).unwrap();
    assert!(!typed_json.contains(download_url));
    assert!(!typed_json.contains(file_id));
    assert!(!typed_json.contains("private-name.pptx"));
}

#[test]
fn apply_patch_audit_records_matching_mode_without_patch_body() {
    let private_patch =
        "*** Begin Patch\n*** Add File: NEVER_LOG_PATCH_BODY.txt\n+secret\n*** End Patch";
    let arguments = serde_json::json!({
        "project": "agent:test:demo",
        "patch": private_patch,
        "dry_run": true,
        "matching_mode": "exact_unique",
    });

    let raw_summary =
        super::super::tool_audit::session_log_arguments_for_tool_request("apply_patch", &arguments);
    assert_eq!(raw_summary["project"], "agent:test:demo");
    assert_eq!(raw_summary["patch_present"], true);
    assert_eq!(raw_summary["dry_run"], true);
    assert_eq!(raw_summary["matching_mode"], "exact_unique");
    assert!(!serde_json::to_string(&raw_summary)
        .unwrap()
        .contains("NEVER_LOG_PATCH_BODY"));

    let call = ToolCall::from_tool_name("apply_patch", arguments).unwrap();
    let typed_summary = call.session_log_arguments();
    assert_eq!(typed_summary["project"], "agent:test:demo");
    assert_eq!(typed_summary["patch_present"], true);
    assert_eq!(typed_summary["dry_run"], true);
    assert_eq!(typed_summary["matching_mode"], "exact_unique");
    assert!(!serde_json::to_string(&typed_summary)
        .unwrap()
        .contains("NEVER_LOG_PATCH_BODY"));
}

#[tokio::test]
async fn conversation_import_durable_session_events_do_not_store_host_file_refs() {
    use crate::tool_runtime::kernel::{
        HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolTransport,
    };

    let runtime = runtime_with_agent_project("telemetry-conversation-import");
    register_agent(
        &runtime,
        "telemetry-conversation-import",
        None,
        RunnerCapabilities {
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("telemetry-conversation-import");
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let download_url = "https://download.example/NEVER_PERSIST_DURABLE_IMPORT_URL";
    let file_id = "NEVER_PERSIST_DURABLE_IMPORT_FILE_ID";
    let auth = auth_context(None, true);
    let outcome = runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: "import_host_files".to_string(),
                arguments: serde_json::json!({
                    "project": project,
                    "openaiFileIdRefs": [{
                        "download_url": download_url,
                        "file_id": file_id,
                        "mime_type": "application/vnd.openxmlformats-officedocument.presentationml.presentation",
                        "file_name": "private-durable-name.pptx"
                    }],
                    "targets": ["import-test.pptx"]
                }),
            },
            ToolCallContext {
                transport: ToolTransport::Mcp,
                session_id: Some(&session.session_id),
                auth: Some(&auth),
                window: None,
                record_oauth_scope_denials: false,
                host_file_import_trust: HostFileImportTrust::Untrusted,
            },
        )
        .await;
    let result = outcome.result.expect("tool result");
    assert!(!result.success);
    assert!(result
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("trusted MCP host-file provenance"));

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    let serialized = serde_json::to_string(&summary.events).unwrap();
    assert!(!serialized.contains(download_url));
    assert!(!serialized.contains(file_id));
    assert!(!serialized.contains("private-durable-name.pptx"));
}

#[tokio::test]
async fn read_project_artifact_metadata_allow_missing_does_not_count_as_failed() {
    let runtime = runtime_with_agent_project("artifact-missing-session");
    register_agent(
        &runtime,
        "artifact-missing-session",
        None,
        RunnerCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("artifact-missing-session");
    let session = runtime.sessions.start_session(Some(project.clone()), None);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadProjectArtifactMetadata {
                        project,
                        path: "artifacts/smoke/missing.artifact".to_string(),
                        session_id: Some(session_id),
                        allow_missing: Some(true),
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });

    let req = wait_for_patch_agent_request(&runtime, "artifact-missing-session").await;
    let payload: serde_json::Value = serde_json::from_str(req.content.as_deref().unwrap()).unwrap();
    assert_eq!(
        payload["max_bytes"],
        super::super::files::MAX_PROJECT_ARTIFACT_EXPORT_BYTES
    );
    complete_patch_agent_request(
        &runtime,
        "artifact-missing-session",
        &req.request_id,
        0,
        r#"{"path":"artifacts/smoke/missing.artifact","exists":false,"missing":true}"#,
        "",
    )
    .await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["exists"], false);
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.failed, 0);
    let event = finished_event(&summary, "read_project_artifact_metadata");
    assert_eq!(event.status.as_deref(), Some("succeeded"));
}

#[tokio::test]
async fn artifact_upload_begin_policy_rejection_is_classified() {
    let runtime = runtime_with_agent_project("artifact-policy-session");
    register_agent(
        &runtime,
        "artifact-policy-session",
        None,
        RunnerCapabilities {
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("artifact-policy-session");
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let bootstrap = auth_context(None, true);

    let result = runtime
        .dispatch_with_auth(
            ToolCall::ArtifactUploadBegin {
                project,
                path: ".env".to_string(),
                session_id: Some(session.session_id.clone()),
                expected_bytes: Some(1),
                expected_sha256: None,
                mime_type: Some("application/octet-stream".to_string()),
                overwrite: Some(false),
            },
            Some(&bootstrap),
        )
        .await;

    assert!(!result.success);
    assert!(result.output.get("permission").is_none());
    assert_eq!(result.output["failure_kind"], "policy_rejected");
    assert_eq!(result.output["error_kind"], "policy_rejected");
    let error = result.error.as_deref().unwrap();
    assert!(error.contains("sensitive artifact path"), "{error}");
    assert!(
        probe_patch_agent_request(&runtime, "artifact-policy-session")
            .await
            .is_none(),
        "policy rejection must happen before enqueue"
    );

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.failed, 1);
    let event = finished_event(&summary, "begin_artifact_upload");
    assert_eq!(event.failure_kind.as_deref(), Some("policy_rejected"));
    assert_eq!(event.error_kind.as_deref(), Some("policy_rejected"));
    assert!(event.permission.is_none());
}

async fn run_list_project_files_page(
    client_id: &str,
    stdout: &str,
    limit: usize,
    offset: usize,
) -> ToolResult {
    let runtime = runtime_with_agent_project(client_id);
    register_agent(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client_id);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .list_project_files(project, None, Some(limit), Some(offset))
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    assert_eq!(request.kind, "file_list");
    assert_eq!(request.path.as_deref(), Some("."));
    complete_patch_agent_request(&runtime, client_id, &request.request_id, 0, stdout, "").await;
    task.await.unwrap()
}

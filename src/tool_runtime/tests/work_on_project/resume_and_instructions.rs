
#[tokio::test]
async fn work_on_project_continues_exact_session_and_appends_instruction() {
    let root = tempfile::tempdir().unwrap();
    let goal_store = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let goal_db = std::sync::Arc::new(
        crate::db::Database::open(&goal_store.path().join("goals.db")).unwrap(),
    );
    let runtime = ToolRuntime::new_for_tests().with_communication_database(goal_db);
    let project =
        register_runner_project_at_path(&runtime, "wop-continue", "demo", root.path()).await;
    let auth = auth_context(None, true);

    let first = dispatch_coding_call_in_window(
        &runtime,
        "wop-continue",
        work_on_project_call(&project, "root objective", None),
        Some(&auth),
        "wop-continue-window",
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let session_id = first.output["session_id"].as_str().unwrap().to_string();
    assert!(
        first.output.get("goal_context").is_none(),
        "fresh Session must not fabricate active Goal context"
    );
    let before = instruction_events(&runtime, &session_id);
    assert_eq!(before.len(), 1);

    let created = runtime.create_goal_with_plan(
        Some(&auth),
        NewGoal {
            title: "Continue exact Goal".into(),
            objective: "Reuse this Goal on normal Workflow Session re-entry.".into(),
            controller_agent_id: None,
            completion_conditions: vec!["Normal re-entry reuses exact Goal identity".into()],
            steps: vec![
                NewGoalStep {
                    id: "inspect".into(),
                    title: "Inspect".into(),
                },
                NewGoalStep {
                    id: "verify".into(),
                    title: "Verify".into(),
                },
            ],
            idempotency_key: "wop-goal-create".into(),
        },
    );
    assert!(created.success, "{:?}", created.output);
    let goal_id = created.output["goal"]["summary"]["goal_id"]
        .as_str()
        .unwrap()
        .to_string();
    let associated = runtime
        .associate_goal_workflow_session(
            Some(&auth),
            goal_id.clone(),
            session_id.clone(),
            "wop-goal-link".into(),
        )
        .await;
    assert!(associated.success, "{:?}", associated.output);
    let checkpoint = runtime.checkpoint_goal(
        Some(&auth),
        goal_id.clone(),
        2,
        crate::db::GoalCheckpoint {
            completed_step_ids: vec!["inspect".into()],
            current_step_id: Some("verify".into()),
            summary: "Inspect complete; verify next.".into(),
        },
        "wop-goal-checkpoint".into(),
    );
    assert!(checkpoint.success, "{:?}", checkpoint.output);

    let continued = dispatch_coding_call_in_window(
        &runtime,
        "wop-continue",
        work_on_project_call("", "follow-up instruction", Some(&session_id)),
        Some(&auth),
        "wop-continue-window",
    )
    .await;
    assert!(continued.success, "{:?}", continued.error);
    assert_eq!(continued.output["session_id"], session_id);
    assert_eq!(continued.output["continuation"], "resumed_explicitly");
    assert_eq!(continued.output["goal_context"]["available"], true);
    assert_eq!(continued.output["goal_context"]["truncated"], false);
    assert_eq!(
        continued.output["goal_context"]["goals"],
        json!([{
            "goal_id": goal_id,
            "revision": 3,
            "incomplete_step_count": 1,
            "current_step": {"id": "verify", "title": "Verify"},
            "next_action": "checkpoint_goal"
        }])
    );
    assert!(first.output.get("workflow").is_none());
    assert!(continued.output.get("workflow").is_none());

    let second = runtime.create_goal(
        Some(&auth),
        "Second active Goal".into(),
        "Remain explicit when multiple active Goals share one Session.".into(),
        "wop-second-goal-create".into(),
    );
    assert!(second.success, "{:?}", second.output);
    let second_goal_id = second.output["goal"]["summary"]["goal_id"]
        .as_str()
        .unwrap()
        .to_string();
    let second_link = runtime
        .associate_goal_workflow_session(
            Some(&auth),
            second_goal_id.clone(),
            session_id.clone(),
            "wop-second-goal-link".into(),
        )
        .await;
    assert!(second_link.success, "{:?}", second_link.output);
    let continued_with_multiple = dispatch_coding_call_in_window(
        &runtime,
        "wop-continue",
        work_on_project_call(&project, "choose explicit active Goal", Some(&session_id)),
        Some(&auth),
        "wop-continue-window",
    )
    .await;
    assert!(
        continued_with_multiple.success,
        "{:?}",
        continued_with_multiple.error
    );
    let goals = continued_with_multiple.output["goal_context"]["goals"]
        .as_array()
        .unwrap();
    assert_eq!(goals.len(), 2);
    let mut returned_ids = goals
        .iter()
        .map(|goal| goal["goal_id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    returned_ids.sort();
    let mut expected_ids = vec![goal_id, second_goal_id];
    expected_ids.sort();
    assert_eq!(returned_ids, expected_ids);
    assert!(
        continued_with_multiple.output["goal_context"]
            .get("selected_goal_id")
            .is_none(),
        "startup Goal context must never auto-select among active Goals"
    );

    // Explicit resume reuses exactly one Session and appends one instruction.
    assert_eq!(
        runtime
            .sessions
            .active_session_count_for_test(Some(&project)),
        1
    );

    // Follow-up instruction appended; root title preserved.
    let events = instruction_events(&runtime, &session_id);
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].instruction.as_deref(), Some("root objective"));
    assert_eq!(
        events[1].instruction.as_deref(),
        Some("follow-up instruction")
    );
    assert_eq!(
        events[2].instruction.as_deref(),
        Some("choose explicit active Goal")
    );
    let summary = runtime.sessions.summary(&session_id, Some(50)).unwrap();
    assert_eq!(summary.title.as_deref(), Some("root objective"));
    assert_eq!(summary.mode, SessionMode::Normal);
    assert!(!summary.guards.deny_write_tools);
    assert!(!summary.guards.deny_shell_tools);
    assert!(
        !serde_json::to_string(&summary)
            .unwrap()
            .contains("webcodex.coding_workflow"),
        "workflow projection must not become Session state"
    );
}

#[tokio::test]
async fn work_on_project_exact_resume_omits_goal_context_without_active_goal() {
    let root = tempfile::tempdir().unwrap();
    let goal_store = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let goal_db = std::sync::Arc::new(
        crate::db::Database::open(&goal_store.path().join("goals.db")).unwrap(),
    );
    let runtime = ToolRuntime::new_for_tests().with_communication_database(goal_db);
    let project =
        register_runner_project_at_path(&runtime, "wop-no-goal", "demo", root.path()).await;
    let auth = auth_context(None, true);

    let first = dispatch_coding_call_in_window(
        &runtime,
        "wop-no-goal",
        work_on_project_call(&project, "root objective", None),
        Some(&auth),
        "wop-no-goal-window",
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let session_id = first.output["session_id"].as_str().unwrap().to_string();

    let resumed = dispatch_coding_call_in_window(
        &runtime,
        "wop-no-goal",
        work_on_project_call(&project, "ordinary continuation", Some(&session_id)),
        Some(&auth),
        "wop-no-goal-window",
    )
    .await;
    assert!(resumed.success, "{:?}", resumed.error);
    assert_eq!(resumed.output["session_id"], session_id);
    assert_eq!(resumed.output["continuation"], "resumed_explicitly");
    assert!(
        resumed.output.get("goal_context").is_none(),
        "exact Session re-entry with zero active Goals must keep startup sparse"
    );
}

#[tokio::test]
async fn work_on_project_failures_never_create_or_fall_back() {
    let dir = tempfile::tempdir().unwrap();
    let root_a = dir.path().join("a");
    let root_b = dir.path().join("b");
    std::fs::create_dir_all(&root_a).unwrap();
    std::fs::create_dir_all(&root_b).unwrap();
    init_git_repo(&root_a);
    init_git_repo(&root_b);
    let runtime = ToolRuntime::new_for_tests();
    register_agent_with_projects(
        &runtime,
        "wop-fail",
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            ..Default::default()
        },
        vec![
            registered_project("a", &root_a.to_string_lossy()),
            registered_project("b", &root_b.to_string_lossy()),
        ],
    )
    .await;
    let project_a = crate::tool_runtime::runner_project_runtime_id("wop-fail", "a");
    let project_b = crate::tool_runtime::runner_project_runtime_id("wop-fail", "b");
    let auth = auth_context(None, true);

    // Create a stable active session on project A, plus a closed one.
    let first = dispatch_coding_call_in_window(
        &runtime,
        "wop-fail",
        work_on_project_call(&project_a, "stable session", None),
        Some(&auth),
        "wop-fail-window",
    )
    .await;
    assert!(first.success);
    let active_id = first.output["session_id"].as_str().unwrap().to_string();
    let closed_id = runtime
        .sessions
        .start_session_with_guards(
            Some(project_a.clone()),
            Some("closed project A".to_string()),
            SessionMode::Normal,
            SessionGuards::default(),
        )
        .session_id;
    runtime.sessions.close_session(&closed_id).unwrap();

    // Unknown Session: no creation, structured unknown_session_id failure.
    let unknown = dispatch_coding_call_in_window(
        &runtime,
        "wop-fail",
        work_on_project_call(
            &project_a,
            "must not create",
            Some("wc_sess_1111111111111111"),
        ),
        Some(&auth),
        "wop-fail-window",
    )
    .await;
    assert!(!unknown.success);
    assert_eq!(unknown.output["error_kind"], "unknown_session_id");

    // Closed Session: no creation, structured session_closed failure.
    let closed = dispatch_coding_call_in_window(
        &runtime,
        "wop-fail",
        work_on_project_call(&project_a, "must not reopen", Some(&closed_id)),
        Some(&auth),
        "wop-fail-window",
    )
    .await;
    assert!(!closed.success);
    assert_eq!(closed.output["error_kind"], "session_closed");
    assert_eq!(closed.output["lifecycle"], "closed");

    // Project mismatch: no fallback to any other session.
    let mismatch = dispatch_coding_call_in_window(
        &runtime,
        "wop-fail",
        work_on_project_call(&project_b, "must not cross", Some(&active_id)),
        Some(&auth),
        "wop-fail-window",
    )
    .await;
    assert!(!mismatch.success);
    assert_eq!(mismatch.output["error_kind"], "session_project_mismatch");
    assert_eq!(mismatch.output["session_project"], project_a);
    assert_eq!(mismatch.output["request_project"], project_b);

    // Invalid Session id fails before execution (no session created).
    let invalid = dispatch_coding_call_in_window(
        &runtime,
        "wop-fail",
        work_on_project_call(&project_a, "must not run", Some("not-a-session")),
        Some(&auth),
        "wop-fail-window",
    )
    .await;
    assert!(!invalid.success);
    assert_eq!(invalid.output["error_kind"], "invalid_session_id");

    // Nothing new was created and the active session is unchanged.
    assert_eq!(
        runtime
            .sessions
            .active_session_count_for_test(Some(&project_a)),
        1
    );
    let events = instruction_events(&runtime, &active_id);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].instruction.as_deref(), Some("stable session"));
}

#[test]
fn finish_coding_task_remains_optional_and_advisory() {
    let specs = registered_tool_specs();
    let names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();
    assert!(names.contains(&"finish_coding_task"), "still public");

    let finish = spec_named(&specs, "finish_coding_task");
    let description = finish.description.to_lowercase();
    for phrase in [
        "optional",
        "advisory",
        "does not decide task completion",
        "generate the user-facing final report",
        "presence does not prove content",
    ] {
        assert!(
            description.contains(phrase),
            "finish_coding_task description must include {phrase}: {description}"
        );
    }
    assert!(
        finish.description.contains("does not"),
        "finish_coding_task description must be explicit about non-authority"
    );

    // The default coding manifest intent does not mark finish as the required
    // final step: it is the last optional evidence snapshot in the list.
    let coding = crate::tool_runtime::tool_definition::TOOL_MANIFEST_INTENTS
        .iter()
        .find(|intent| intent.name == "coding")
        .expect("coding intent");
    assert!(coding.tools.contains(&"work_on_project"));
    assert!(coding.tools.contains(&"finish_coding_task"));
    assert!(
        coding
            .tools
            .iter()
            .position(|t| *t == "finish_coding_task")
            .unwrap()
            > coding
                .tools
                .iter()
                .position(|t| *t == "work_on_project")
                .unwrap()
    );
}

/// Seed a representative Rust-style repository for the startup overview. The
/// files are committed so the tracked git index (the overview's project
/// boundary) includes every fixture entry; sensitive/build paths stay
/// excluded by the overview's own path policy.
fn seed_coding_repository(root: &std::path::Path, agents_body: &str) {
    init_git_repo(root);
    std::fs::write(
        root.join("AGENTS.md"),
        format!("# Repository rules\n\n{agents_body}\n"),
    )
    .unwrap();
    for path in [
        "README.md",
        "Cargo.toml",
        "src/lib.rs",
        "tests/basic.rs",
        "docs/index.md",
        "scripts/check.sh",
        ".github/workflows/ci.yml",
        "src/generated/deep/path.rs",
    ] {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, b"fixture contents must never be read").unwrap();
    }
    // Untracked/build/sensitive paths must never appear in the overview.
    std::fs::write(root.join(".env"), b"SECRET=do-not-leak").unwrap();
    std::fs::create_dir_all(root.join("target/debug")).unwrap();
    std::fs::write(root.join("target/debug/output"), b"binary").unwrap();
    for cmd in [
        "git add -A",
        "git commit -m 'seed fixture'",
        "git config status.showUntrackedFiles all",
    ] {
        let (exit_code, stdout, stderr, _) =
            crate::tool_runtime::helpers::run_command_sync(cmd, root, 30);
        assert_eq!(exit_code, 0, "{cmd}\n{stdout}{stderr}");
    }
}

/// Overwrite `AGENTS.md` in place (still tracked) so a follow-up resume sees a
/// changed fingerprint without a commit.
fn overwrite_agents_rule(root: &std::path::Path, body: &str) {
    std::fs::write(
        root.join("AGENTS.md"),
        format!("# Repository rules\n\n{body}\n"),
    )
    .unwrap();
}

#[tokio::test]
async fn work_on_project_new_task_is_lightweight_and_preserves_startup_context() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "Preserve unrelated changes.");
    let runtime = ToolRuntime::new_for_tests();
    register_agent_with_projects(
        &runtime,
        "wop-repo",
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            lsp_read_only_navigation: true,
            internal_posix_script: true,
            ..Default::default()
        },
        vec![registered_project("demo", &root.path().to_string_lossy())],
    )
    .await;
    let project = crate::tool_runtime::runner_project_runtime_id("wop-repo", "demo");
    let auth = auth_context(None, true);

    let (result, request_kinds) = dispatch_recording_startup_requests(
        &runtime,
        "wop-repo",
        work_on_project_call(&project, "start on the repository", None),
        Some(&auth),
        "wop-repo-window",
    )
    .await;
    assert!(result.success, "{:?}", result.error);

    // resolved_project is the full runtime project id.
    assert_eq!(result.output["resolved_project"], project);
    assert_eq!(result.output["workspace"]["upstream_status"], "absent");
    assert!(result.output["workspace"].get("upstream").is_none());
    assert!(result.output["workspace"].get("ahead").is_none());
    assert!(result.output["workspace"].get("behind").is_none());
    // Neither an inconclusive LSP probe nor an intentionally skipped
    // repository overview is a readiness warning.
    assert!(result.output.get("repository").is_none());
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

    // Runner request evidence: rules, Git, and LSP probes remain; repository
    // overview is not merely hidden from JSON, it is never enqueued.
    assert!(
        request_kinds.iter().any(|kind| kind == "file_read"),
        "repository rules were not observed: {request_kinds:?}"
    );
    assert!(
        request_kinds
            .iter()
            .any(|kind| kind == "run_internal_posix_script"),
        "Git/workspace inspection was not executed through the internal POSIX runtime: {request_kinds:?}"
    );
    assert!(
        request_kinds
            .iter()
            .any(|kind| kind == AGENT_LSP_REQUEST_KIND),
        "semantic navigation was not probed: {request_kinds:?}"
    );
    assert!(
        request_kinds
            .iter()
            .all(|kind| kind != "file_project_overview"),
        "work_on_project unexpectedly enqueued an overview: {request_kinds:?}"
    );
    assert!(
        request_kinds
            .iter()
            .all(|kind| kind != RUNNER_INSTRUCTION_REQUEST_KIND),
        "an older Runner without instruction_runtime must not receive the new request: {request_kinds:?}"
    );

    // Instructions are still observed, but the primary projection is metadata-only.
    let instructions = &result.output["instructions"];
    assert_eq!(instructions["status"], "loaded");
    assert!(instructions.get("content_included").is_none());
    assert!(instructions["sources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| source["path"] == "AGENTS.md"
            && source["fingerprint"].is_string()
            && source.get("content").is_none()));

    // Semantic navigation block exists and is deterministic.
    assert!(result.output["semantic_navigation"].is_object());
    assert!(result.output["semantic_navigation"]["status"].is_string());

    // No noteworthy Job state means no jobs block at all.
    assert!(result.output.get("jobs").is_none());

    // No full diagnostics leak.
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

    // Exactly one fresh Session exists.
    assert_eq!(
        runtime
            .sessions
            .active_session_count_for_test(Some(&project)),
        1
    );

    // Schema validates.
    let schema = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
    let instance = json!({ "success": true, "output": result.output });
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&instance, &schema)
        .unwrap_or_else(|error| panic!("compact output must match its schema: {error}"));
    let bytes = serde_json::to_vec(&result.output).unwrap().len();
    assert!(bytes <= crate::tool_runtime::startup_brief::STANDARD_STARTUP_HARD_MAX_BYTES);
    assert!(
        !result
            .output
            .to_string()
            .contains(&root.path().to_string_lossy().to_string()),
        "compact output leaked the absolute repository path"
    );
}

#[tokio::test]
async fn runner_global_instructions_compose_change_and_repeat_across_projects() {
    let root_a = tempfile::tempdir().unwrap();
    let root_b = tempfile::tempdir().unwrap();
    seed_coding_repository(root_a.path(), "project A rule");
    init_git_repo(root_b.path());

    let runtime = ToolRuntime::new_for_tests();
    register_agent_with_projects(
        &runtime,
        "wop-global",
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            lsp_read_only_navigation: true,
            internal_posix_script: true,
            instruction_runtime: true,
            ..Default::default()
        },
        vec![
            registered_project("a", &root_a.path().to_string_lossy()),
            registered_project("b", &root_b.path().to_string_lossy()),
        ],
    )
    .await;
    let project_a = crate::tool_runtime::runner_project_runtime_id("wop-global", "a");
    let project_b = crate::tool_runtime::runner_project_runtime_id("wop-global", "b");
    let auth = auth_context(None, true);
    let global_v1 = runner_instruction_snapshot_stdout("runner global v1", 7);

    let (first, first_requests) = dispatch_recording_startup_requests_with_runner_instructions(
        &runtime,
        "wop-global",
        work_on_project_call(&project_a, "project A task", None),
        Some(&auth),
        "wop-global-window",
        &global_v1,
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    assert!(first_requests
        .iter()
        .any(|kind| kind == RUNNER_INSTRUCTION_REQUEST_KIND));
    let first_sources = first.output["instructions"]["sources"].as_array().unwrap();
    assert_eq!(first_sources[0]["source_scope"], "runner");
    assert_eq!(first_sources[0]["path"], "runner/0/AGENTS.md");
    assert!(first_sources[0].get("content").is_none());
    assert_eq!(first_sources[1]["source_scope"], "project");
    assert_eq!(first_sources[1]["path"], "AGENTS.md");
    assert!(first_sources[1].get("content").is_none());
    let first_runner_fingerprint = first_sources[0]["fingerprint"]
        .as_str()
        .unwrap()
        .to_string();
    let first_session_id = first.output["session_id"].as_str().unwrap().to_string();

    let durable_summary = runtime
        .sessions
        .summary(&first_session_id, Some(20))
        .unwrap()
        .project_instructions
        .expect("instruction summary");
    let durable_json = serde_json::to_string(&durable_summary).unwrap();
    assert!(durable_json.contains("runner/0/AGENTS.md"));
    assert!(!durable_json.contains("runner global v1"));
    assert!(!durable_json.contains("project A rule"));

    // The same ChatGPT window opening another Project must observe the Runner-global
    // source again; v1 intentionally has no cross-Project model-context suppression.
    let (second_project, second_requests) =
        dispatch_recording_startup_requests_with_runner_instructions(
            &runtime,
            "wop-global",
            work_on_project_call(&project_b, "project B task", None),
            Some(&auth),
            "wop-global-window",
            &global_v1,
        )
        .await;
    assert!(second_project.success, "{:?}", second_project.error);
    assert!(second_requests
        .iter()
        .any(|kind| kind == RUNNER_INSTRUCTION_REQUEST_KIND));
    let second_sources = second_project.output["instructions"]["sources"]
        .as_array()
        .unwrap();
    assert_eq!(second_sources.len(), 1);
    assert_eq!(second_sources[0]["source_scope"], "runner");
    assert!(second_sources[0].get("content").is_none());

    // Explicit body suppression remains one shared instruction projection switch;
    // it does not create a special retention protocol for Runner-global sources.
    let (suppressed, suppressed_requests) =
        dispatch_recording_startup_requests_with_runner_instructions(
            &runtime,
            "wop-global",
            work_on_project_call(&project_b, "project B metadata-only task", None),
            Some(&auth),
            "wop-global-window",
            &global_v1,
        )
        .await;
    assert!(suppressed.success, "{:?}", suppressed.error);
    assert!(suppressed_requests
        .iter()
        .any(|kind| kind == RUNNER_INSTRUCTION_REQUEST_KIND));
    let suppressed_runner = suppressed.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["source_scope"] == "runner")
        .unwrap();
    assert!(suppressed_runner.get("content").is_none());

    // File contents are live independently from config generation. Even a stale or
    // malformed Runner response that reuses the prior upstream fingerprint cannot
    // hide different visible content from Server-side continuation detection.
    let mut global_v1_wire: serde_json::Value = serde_json::from_str(&global_v1).unwrap();
    let mut global_v2_wire: serde_json::Value =
        serde_json::from_str(&runner_instruction_snapshot_stdout("runner global v2", 7)).unwrap();
    global_v2_wire["files"][0]["fingerprint"] = global_v1_wire["files"][0]["fingerprint"].take();
    let global_v2 = global_v2_wire.to_string();
    let (changed, changed_requests) = dispatch_recording_startup_requests_with_runner_instructions(
        &runtime,
        "wop-global",
        work_on_project_call(
            &project_a,
            "resume after global edit",
            Some(&first_session_id),
        ),
        Some(&auth),
        "wop-global-window",
        &global_v2,
    )
    .await;
    assert!(changed.success, "{:?}", changed.error);
    assert!(changed_requests
        .iter()
        .any(|kind| kind == RUNNER_INSTRUCTION_REQUEST_KIND));
    assert_eq!(changed.output["instructions"]["status"], "changed");
    assert!(changed.output["instructions"]["changed_sources"]
        .as_array()
        .unwrap()
        .contains(&json!("runner/0/AGENTS.md")));
    let changed_runner = changed.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["source_scope"] == "runner")
        .unwrap();
    assert!(changed_runner.get("content").is_none());
    assert_ne!(changed_runner["fingerprint"], first_runner_fingerprint);
}

#[tokio::test]
async fn work_on_project_omits_instruction_bodies_even_for_a_fresh_session() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "caller already knows this rule");
    let runtime = ToolRuntime::new_for_tests();
    let project = register_runner_project_at_path(
        &runtime,
        "wop-instruction-projection",
        "demo",
        root.path(),
    )
    .await;
    let auth = auth_context(None, true);

    let first = dispatch_coding_call_in_window(
        &runtime,
        "wop-instruction-projection",
        work_on_project_call(&project, "first task", None),
        Some(&auth),
        "wop-instruction-projection-window",
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    assert!(first.output["instructions"]
        .get("content_included")
        .is_none());
    let first_session_id = first.output["session_id"].as_str().unwrap().to_string();

    let (second, request_kinds) = dispatch_recording_startup_requests(
        &runtime,
        "wop-instruction-projection",
        work_on_project_call(&project, "second independent task", None),
        Some(&auth),
        "wop-instruction-projection-window",
    )
    .await;
    assert!(second.success, "{:?}", second.error);
    let second_session_id = second.output["session_id"].as_str().unwrap().to_string();
    assert_ne!(second_session_id, first_session_id);
    assert_eq!(second.output["continuation"], "created");

    let instructions = &second.output["instructions"];
    assert_eq!(instructions["status"], "loaded");
    assert!(instructions.get("content_included").is_none());
    let agents_source = instructions["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["path"] == "AGENTS.md")
        .expect("AGENTS.md metadata");
    assert!(agents_source.get("content").is_none());
    assert!(agents_source.get("headings").is_none());
    assert!(agents_source.get("truncated").is_none());
    assert!(agents_source["fingerprint"]
        .as_str()
        .is_some_and(|value| value.len() == 64));
    assert!(
        request_kinds.iter().any(|kind| kind == "file_read"),
        "instruction files must still be observed when their bodies are omitted: {request_kinds:?}"
    );
    let schema = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
    let instance = json!({ "success": true, "output": second.output.clone() });
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&instance, &schema)
        .unwrap_or_else(|error| {
            panic!("sparse instruction metadata must match output schema: {error}")
        });

    let summary = runtime
        .sessions
        .summary(&second_session_id, Some(20))
        .unwrap();
    let snapshot = summary
        .project_instructions
        .expect("fresh Workflow Session instruction summary");
    assert!(snapshot.loaded);
    let stored_agents = snapshot
        .files
        .iter()
        .find(|file| file.path == "AGENTS.md")
        .expect("stored AGENTS.md summary");
    assert_eq!(
        Some(stored_agents.fingerprint.as_str()),
        agents_source["fingerprint"].as_str()
    );
}

#[tokio::test]
async fn work_on_project_fresh_window_discovers_only_owning_runner_acp_and_admits_it() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "Review source without changing files");
    let runtime = ToolRuntime::new_for_tests();
    let provider = |id: &str, name: &str| webcodex_core::coding_agent::CodingAgentProvider {
        provider_id: id.to_owned(),
        name: name.to_owned(),
        provider_instance_id: format!("private-provider-{id}"),
    };
    let project = register_runner_project_at_path_with_coding_agents(
        &runtime,
        "wop-acp",
        "demo",
        root.path(),
        Some(vec![provider("pi", "Pi Agent")]),
    )
    .await;
    register_runner_project_at_path_with_coding_agents(
        &runtime,
        "other-acp",
        "other",
        root.path(),
        Some(vec![provider("codex", "Codex Agent")]),
    )
    .await;
    let auth = auth_context(None, true);
    for window in ["fresh-acp-window-a", "fresh-acp-window-b"] {
        let result = dispatch_coding_call_in_window(
            &runtime,
            "wop-acp",
            work_on_project_call(
                &project,
                "Review the repository without changing files",
                None,
            ),
            Some(&auth),
            window,
        )
        .await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(
            result.output["coding_agent_providers"],
            json!([{"provider_id":"pi","name":"Pi Agent"}])
        );
        assert!(!result.output.to_string().contains("private-provider-"));
        let advertised = result.output["coding_agent_providers"][0]["provider_id"]
            .as_str()
            .unwrap();
        assert!(runtime
            .prepare_coding_agent_start(
                project.clone(),
                advertised.to_owned(),
                format!("discover-{window}"),
                "Read-only module review".to_owned(),
                None,
                Some(10),
                Some(&auth),
            )
            .await
            .is_ok());
        let schema = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &json!({"success": true, "output": result.output}),
            &schema,
        )
        .unwrap();
    }
}

#[tokio::test]
async fn work_on_project_omits_static_guidance_from_primary_output() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "keep repository guidance visible");
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_runner_project_at_path(&runtime, "wop-workflow-projection", "demo", root.path())
            .await;
    let auth = auth_context(None, true);

    let result = dispatch_coding_call_in_window(
        &runtime,
        "wop-workflow-projection",
        work_on_project_call(&project, "caller already knows the static workflow", None),
        Some(&auth),
        "wop-workflow-projection-window",
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert!(result.output.get("workflow").is_none());
    assert!(result.output["instructions"]
        .get("content_included")
        .is_none());
    assert!(result.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .all(|source| source.get("content").is_none()));

    let session_id = result.output["session_id"].as_str().unwrap();
    let summary = runtime.sessions.summary(session_id, Some(20)).unwrap();
    assert_eq!(summary.project.as_deref(), Some(project.as_str()));

    let schema = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
    let instance = json!({ "success": true, "output": result.output });
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&instance, &schema)
        .unwrap_or_else(|error| panic!("workflow-omitted output must match schema: {error}"));
}

#[tokio::test]
async fn work_on_project_static_projection_is_not_inferred_from_window_or_session_state() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "static caller-explicit rule");
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_runner_project_at_path(&runtime, "wop-explicit", "demo", root.path()).await;
    let auth = auth_context(None, true);

    let first = dispatch_coding_call_in_window(
        &runtime,
        "wop-explicit",
        work_on_project_call(&project, "first", None),
        Some(&auth),
        "same-window",
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let session_id = first.output["session_id"].as_str().unwrap().to_string();
    assert!(first.output.get("workflow").is_none());

    let repeated = dispatch_coding_call_in_window(
        &runtime,
        "wop-explicit",
        work_on_project_call(&project, "repeat true", Some(&session_id)),
        Some(&auth),
        "same-window",
    )
    .await;
    assert!(repeated.success, "{:?}", repeated.error);
    assert!(repeated.output.get("workflow").is_none());
    assert_eq!(repeated.output["instructions"]["status"], "reused");
    assert!(repeated.output["instructions"]
        .get("content_included")
        .is_none());

    let suppressed = dispatch_coding_call_in_window(
        &runtime,
        "wop-explicit",
        work_on_project_call(
            &project,
            "caller suppresses static content",
            Some(&session_id),
        ),
        Some(&auth),
        "same-window",
    )
    .await;
    assert!(suppressed.success, "{:?}", suppressed.error);
    assert!(suppressed.output.get("workflow").is_none());
    assert_eq!(suppressed.output["instructions"]["status"], "reused");
    assert!(suppressed.output["instructions"]
        .get("content_included")
        .is_none());
    let suppressed_agents = suppressed.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["path"] == "AGENTS.md")
        .unwrap();
    assert!(suppressed_agents["fingerprint"].is_string());
    assert!(suppressed_agents.get("content").is_none());
    assert!(suppressed_agents.get("headings").is_none());
    assert!(suppressed_agents.get("read_more").is_none());

    let restored_other_window = dispatch_coding_call_in_window(
        &runtime,
        "wop-explicit",
        work_on_project_call(&project, "true in another window", Some(&session_id)),
        Some(&auth),
        "different-window",
    )
    .await;
    assert!(
        restored_other_window.success,
        "{:?}",
        restored_other_window.error
    );
    assert!(restored_other_window.output.get("workflow").is_none());
    assert!(restored_other_window.output["instructions"]
        .get("content_included")
        .is_none());

    let no_window = dispatch_startup_without_window(
        &runtime,
        "wop-explicit",
        work_on_project_call(&project, "true without window", Some(&session_id)),
        Some(&auth),
    )
    .await;
    assert!(no_window.success, "{:?}", no_window.error);
    assert!(no_window.output.get("workflow").is_none());
    assert!(no_window.output["instructions"]
        .get("content_included")
        .is_none());
}

#[tokio::test]
async fn work_on_project_suppressed_instruction_bodies_still_track_changed_rules() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "old body");
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_runner_project_at_path(&runtime, "wop-suppressed-change", "demo", root.path())
            .await;
    let auth = auth_context(None, true);

    let first = dispatch_coding_call_in_window(
        &runtime,
        "wop-suppressed-change",
        work_on_project_call(&project, "first", None),
        Some(&auth),
        "window-a",
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let session_id = first.output["session_id"].as_str().unwrap().to_string();
    let old_fingerprint = first.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["path"] == "AGENTS.md")
        .unwrap()["fingerprint"]
        .as_str()
        .unwrap()
        .to_string();

    let long_changed_body = std::iter::once("new body while projection is suppressed".to_string())
        .chain((0..500).map(|index| format!("suppressed-line-{index}")))
        .collect::<Vec<_>>()
        .join("\n");
    overwrite_agents_rule(root.path(), &long_changed_body);
    let changed = dispatch_coding_call_in_window(
        &runtime,
        "wop-suppressed-change",
        work_on_project_call(&project, "observe change without body", Some(&session_id)),
        Some(&auth),
        "window-a",
    )
    .await;
    assert!(changed.success, "{:?}", changed.error);
    assert_eq!(changed.output["instructions"]["status"], "changed");
    assert!(changed.output["instructions"]["changed_sources"]
        .as_array()
        .unwrap()
        .contains(&json!("AGENTS.md")));
    let changed_agents = changed.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["path"] == "AGENTS.md")
        .unwrap();
    assert_ne!(changed_agents["fingerprint"], old_fingerprint);
    assert!(changed_agents.get("content").is_none());
    assert!(changed_agents.get("headings").is_none());
    assert!(changed_agents.get("read_more").is_none());

    let projected = dispatch_coding_call_in_window(
        &runtime,
        "wop-suppressed-change",
        work_on_project_call(&project, "project current body", Some(&session_id)),
        Some(&auth),
        "window-b",
    )
    .await;
    assert!(projected.success, "{:?}", projected.error);
    assert_eq!(projected.output["instructions"]["status"], "reused");
    assert!(projected.output["instructions"]
        .get("content_included")
        .is_none());
    assert!(projected.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .all(|source| source.get("content").is_none()));
}

#[tokio::test]
async fn work_on_project_exact_resume_reuses_rules_and_detects_changes() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "first rule body");
    let runtime = ToolRuntime::new_for_tests();
    let project = register_runner_project_at_path(&runtime, "wop-reuse", "demo", root.path()).await;
    let auth = auth_context(None, true);

    let first = dispatch_coding_call_in_window(
        &runtime,
        "wop-reuse",
        work_on_project_call(&project, "root objective", None),
        Some(&auth),
        "wop-reuse-window",
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let session_id = first.output["session_id"].as_str().unwrap().to_string();

    // Exact resume with unchanged rules keeps delta metadata while static bodies remain opt-in.
    let reused = dispatch_coding_call_in_window(
        &runtime,
        "wop-reuse",
        work_on_project_call(&project, "follow-up", Some(&session_id)),
        Some(&auth),
        "wop-reuse-window",
    )
    .await;
    assert!(reused.success, "{:?}", reused.error);
    assert_eq!(reused.output["session_id"], session_id);
    assert_eq!(reused.output["continuation"], "resumed_explicitly");
    let reused_instructions = &reused.output["instructions"];
    assert_eq!(reused_instructions["status"], "reused");
    assert!(reused_instructions.get("content_included").is_none());
    assert!(reused_instructions.get("changed_sources").is_none());
    let reused_agents = reused_instructions["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["path"] == "AGENTS.md")
        .expect("reused AGENTS.md source");
    assert!(reused_agents.get("content").is_none());
    assert!(reused_agents.get("headings").is_none());
    assert!(reused_agents["fingerprint"].is_string());

    // Change the rule then resume: status=changed, changed_sources includes it.
    overwrite_agents_rule(root.path(), "changed rule body");
    let changed = dispatch_coding_call_in_window(
        &runtime,
        "wop-reuse",
        work_on_project_call(&project, "after rule change", Some(&session_id)),
        Some(&auth),
        "wop-reuse-window",
    )
    .await;
    assert!(changed.success, "{:?}", changed.error);
    assert_eq!(changed.output["session_id"], session_id);
    assert_eq!(changed.output["instructions"]["status"], "changed");
    assert!(
        changed.output["instructions"]["changed_sources"]
            .as_array()
            .unwrap()
            .contains(&json!("AGENTS.md")),
        "{:?}",
        changed.output["instructions"]["changed_sources"]
    );
    assert!(changed.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| source["path"] == "AGENTS.md"
            && source["fingerprint"].is_string()
            && source.get("content").is_none()));
}

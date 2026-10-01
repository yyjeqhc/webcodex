#[tokio::test]
async fn review_snapshot_identity_fences_index_only_changes() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    fs::write(tmp.path().join("a.txt"), "base\n").unwrap();
    git_test_command_ok(tmp.path(), "git add a.txt && git commit -m base");
    fs::write(tmp.path().join("a.txt"), "worktree\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "review-index-fence", "repo", tmp.path())
            .await;
    let index_before = fs::read(tmp.path().join(".git/index")).unwrap();
    let unstaged = observe_review_source(&runtime, &project).await;
    assert_eq!(
        index_before,
        fs::read(tmp.path().join(".git/index")).unwrap()
    );
    assert_eq!(unstaged, observe_review_source(&runtime, &project).await);
    git_test_command_ok(tmp.path(), "git add a.txt");
    let staged = observe_review_source(&runtime, &project).await;
    assert_eq!(unstaged["head_commit"], staged["head_commit"]);
    assert_eq!(unstaged["frozen_tree"], staged["frozen_tree"]);
    assert_ne!(
        unstaged, staged,
        "staging must invalidate cached review metadata"
    );

    fs::write(tmp.path().join("a.txt"), "index-one\n").unwrap();
    git_test_command_ok(tmp.path(), "git add a.txt");
    fs::write(tmp.path().join("a.txt"), "worktree\n").unwrap();
    let first = observe_review_source(&runtime, &project).await;
    fs::write(tmp.path().join("a.txt"), "index-two\n").unwrap();
    git_test_command_ok(tmp.path(), "git add a.txt");
    fs::write(tmp.path().join("a.txt"), "worktree\n").unwrap();
    let second = observe_review_source(&runtime, &project).await;
    assert_eq!(first["frozen_tree"], second["frozen_tree"]);
    assert_ne!(
        first, second,
        "equal MM status still needs staged object identity"
    );
}

#[tokio::test]
async fn review_snapshot_status_fingerprint_keeps_large_status_bounded() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    for n in 0..1024 {
        fs::write(
            tmp.path().join(format!("{n:04}-{}.txt", "x".repeat(96))),
            "content\n",
        )
        .unwrap();
    }
    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "review-large-status", "repo", tmp.path())
            .await;
    let source = observe_review_source(&runtime, &project).await;
    assert!(
        source["head_commit"].is_null(),
        "unborn HEAD stays supported"
    );
    assert!(source["status_fingerprint"].as_str().is_some());
    assert!(serde_json::to_vec(&source).unwrap().len() < 512);
    assert!(
        !tmp.path().join(".git/index").exists(),
        "observation must not create the real index"
    );
}

#[tokio::test]
async fn review_snapshot_identity_fences_branch_only_changes() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    fs::write(tmp.path().join("a.txt"), "base\n").unwrap();
    git_test_command_ok(tmp.path(), "git add a.txt && git commit -m base");
    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "review-branch-fence", "repo", tmp.path())
            .await;
    let before = observe_review_source(&runtime, &project).await;
    git_test_command_ok(tmp.path(), "git switch -c review-other-branch");
    let after = observe_review_source(&runtime, &project).await;
    assert_eq!(before["head_commit"], after["head_commit"]);
    assert_eq!(before["frozen_tree"], after["frozen_tree"]);
    assert_ne!(
        before, after,
        "branch changes must invalidate cached branch metadata"
    );
}

fn start_review_page(
    runtime: &ToolRuntime,
    project: &str,
    continuation: Option<String>,
) -> tokio::task::JoinHandle<ToolResult> {
    let runtime = runtime.clone();
    let project = project.to_string();
    tokio::spawn(async move {
        runtime
            .review_changes(
                project,
                webcodex_tool_contracts::tool_call::GitReviewScopeInput::Workspace,
                None,
                None,
                Some(1),
                None,
                None,
                continuation,
                None,
            )
            .await
    })
}

fn review_paging_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(tmp.path().join(name), "base\n").unwrap();
    }
    git_test_command_ok(
        tmp.path(),
        "git add a.txt b.txt c.txt && git commit -m base",
    );
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(tmp.path().join(name), "changed\n").unwrap();
    }
    tmp
}

#[tokio::test]
async fn review_snapshot_next_call_omits_absent_optional_arguments() {
    let tmp = review_paging_repo();
    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "review-next-call", "repo", tmp.path())
            .await;
    let schema = registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "review_changes")
        .unwrap()
        .input_schema;
    let mut continuation = None;
    for page in 0..3 {
        let task = start_review_page(&runtime, &project, continuation);
        let result = collect_review_task(&runtime, "review-next-call", task).await;
        assert!(result.success, "{result:?}");
        if page == 2 {
            assert!(result.output["continuation"].is_null(), "{result:?}");
            break;
        }
        let arguments = &result.output["next_call"]["arguments"];
        for (key, value) in arguments.as_object().unwrap() {
            assert!(
                !value.is_null(),
                "generated next_call violates non-null schema: {key}"
            );
        }
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(arguments, &schema)
            .unwrap_or_else(|error| panic!("next_call is not directly callable: {error}"));
        ToolCall::from_tool_name("review_changes", arguments.clone()).unwrap();
        continuation = Some(arguments["continuation"].as_str().unwrap().to_string());
    }
}

#[tokio::test]
async fn review_snapshot_continuation_rechecks_source_after_diff() {
    let tmp = review_paging_repo();
    let runtime = test_runtime();
    let client = "review-page-race";
    let project = register_structured_git_agent_at_path(&runtime, client, "repo", tmp.path()).await;
    let first = collect_review_task(
        &runtime,
        client,
        start_review_page(&runtime, &project, None),
    )
    .await;
    assert!(first.success, "{first:?}");
    let continuation = first.output["continuation"].as_str().unwrap().to_string();
    let next = start_review_page(&runtime, &project, Some(continuation));
    let before = wait_for_patch_agent_request(&runtime, client).await;
    assert!(before
        .script
        .as_ref()
        .unwrap()
        .script
        .contains("WEBCODEX_WORKSPACE_STATUS="));
    complete_agent_request_by_running_locally(&runtime, client, before).await;
    let diff = wait_for_patch_agent_request(&runtime, client).await;
    let (exit_code, stdout, stderr) = run_runner_shell_request_locally(&diff);
    assert_eq!(exit_code, 0, "{stderr}");
    // Change only the branch after the page was produced, before its response
    // reaches Runtime. Neither the worktree nor the inner diff cursor changes.
    git_test_command_ok(tmp.path(), "git switch -c review-concurrent-branch");
    complete_patch_agent_request(
        &runtime,
        client,
        &diff.request_id,
        exit_code,
        &stdout,
        &stderr,
    )
    .await;
    let result = collect_review_task(&runtime, client, next).await;
    assert!(!result.success, "{result:?}");
    assert_eq!(result.output["reason_code"], "snapshot_stale");
    assert!(result.output["diff"].is_null());
}

async fn run_runner_git_commit_paths(
    runtime: &ToolRuntime,
    client_id: &str,
    project: String,
    expected_head: String,
    paths: Vec<String>,
    message: &str,
) -> ToolResult {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let message = message.to_string();
        async move {
            runtime
                .git_commit_paths(project, expected_head, paths, message)
                .await
        }
    });
    let request = wait_for_patch_agent_request(runtime, client_id).await;
    assert_eq!(request.kind, "run_internal_posix_script");
    let script = request
        .script
        .as_ref()
        .expect("commit_git_paths must use a typed internal script");
    assert_eq!(script.language.as_str(), "sh");
    assert!(script.script.contains("git update-ref"));
    assert!(script.script.contains("git commit-tree"));
    assert!(!script.script.contains("git push"));
    let (exit_code, stdout, stderr) = run_runner_shell_request_locally(&request);
    complete_patch_agent_request(
        runtime,
        client_id,
        &request.request_id,
        exit_code,
        &stdout,
        &stderr,
    )
    .await;
    task.await.unwrap()
}

#[tokio::test]
async fn git_commit_paths_commits_only_requested_paths_and_preserves_other_worktree_changes() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    fs::write(tmp.path().join("a.txt"), "base-a\n").unwrap();
    fs::write(tmp.path().join("b.txt"), "base-b\n").unwrap();
    git_test_command_ok(tmp.path(), "git add a.txt b.txt");
    git_test_command_ok(tmp.path(), "git commit -m base");
    let (_, stdout, _, _) = run_command_sync("git rev-parse HEAD", tmp.path(), 30);
    let base = stdout.trim().to_string();

    fs::write(tmp.path().join("a.txt"), "committed-a\n").unwrap();
    fs::write(tmp.path().join("b.txt"), "still-dirty-b\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "commit-paths", "repo", tmp.path()).await;
    let result = run_runner_git_commit_paths(
        &runtime,
        "commit-paths",
        project,
        base.clone(),
        vec!["a.txt".to_string()],
        "commit exact a",
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["committed"], true);
    assert_eq!(result.output["previous_head"], base);
    assert_eq!(result.output["committed_paths"], json!(["a.txt"]));
    assert_eq!(result.output["hook_policy"], "bypassed_exact_tree");

    let new_head = result.output["new_head"].as_str().unwrap();
    let (_, changed, _, _) = run_command_sync(
        &format!("git diff --name-only {} {}", base, new_head),
        tmp.path(),
        30,
    );
    assert_eq!(changed.trim(), "a.txt");
    let (_, staged, _, _) = run_command_sync("git diff --cached --name-only", tmp.path(), 30);
    assert!(
        staged.trim().is_empty(),
        "real index must remain clean: {staged}"
    );
    let (_, status, _, _) = run_command_sync("git status --short", tmp.path(), 30);
    assert_eq!(status.trim(), "M b.txt");
}

#[tokio::test]
async fn git_commit_paths_rejects_existing_staged_state_without_advancing_head() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    fs::write(tmp.path().join("a.txt"), "base-a\n").unwrap();
    fs::write(tmp.path().join("b.txt"), "base-b\n").unwrap();
    git_test_command_ok(tmp.path(), "git add a.txt b.txt");
    git_test_command_ok(tmp.path(), "git commit -m base");
    let (_, stdout, _, _) = run_command_sync("git rev-parse HEAD", tmp.path(), 30);
    let base = stdout.trim().to_string();
    fs::write(tmp.path().join("a.txt"), "staged-a\n").unwrap();
    fs::write(tmp.path().join("b.txt"), "requested-b\n").unwrap();
    git_test_command_ok(tmp.path(), "git add a.txt");

    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "commit-staged-reject", "repo", tmp.path())
            .await;
    let result = run_runner_git_commit_paths(
        &runtime,
        "commit-staged-reject",
        project,
        base.clone(),
        vec!["b.txt".to_string()],
        "must reject staged",
    )
    .await;
    assert!(!result.success);
    assert_eq!(result.output["failure_kind"], "existing_staged");
    assert_eq!(result.output["state_changed"], false);
    let (_, head, _, _) = run_command_sync("git rev-parse HEAD", tmp.path(), 30);
    assert_eq!(head.trim(), base);
    let (_, staged, _, _) = run_command_sync("git diff --cached --name-only", tmp.path(), 30);
    assert_eq!(staged.trim(), "a.txt");
}

#[tokio::test]
async fn git_commit_paths_rejects_stale_expected_head_before_mutation() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    fs::write(tmp.path().join("a.txt"), "base\n").unwrap();
    git_test_command_ok(tmp.path(), "git add a.txt");
    git_test_command_ok(tmp.path(), "git commit -m base");
    let (_, stdout, _, _) = run_command_sync("git rev-parse HEAD", tmp.path(), 30);
    let actual = stdout.trim().to_string();
    fs::write(tmp.path().join("a.txt"), "dirty\n").unwrap();

    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "commit-head-fence", "repo", tmp.path())
            .await;
    let stale = "f".repeat(40);
    let result = run_runner_git_commit_paths(
        &runtime,
        "commit-head-fence",
        project,
        stale.clone(),
        vec!["a.txt".to_string()],
        "must not commit",
    )
    .await;
    assert!(!result.success);
    assert_eq!(result.output["failure_kind"], "head_mismatch");
    assert_eq!(result.output["expected_head"], stale);
    assert_eq!(result.output["actual_head"], actual);
    assert_eq!(result.output["state_changed"], false);
    let (_, head, _, _) = run_command_sync("git rev-parse HEAD", tmp.path(), 30);
    assert_eq!(head.trim(), actual);
}

#[test]
fn git_commit_paths_audit_keeps_message_private_and_exact_head_bounded() {
    let private_message = "PRIVATE_COMMIT_MESSAGE_MUST_NOT_PERSIST";
    let expected_head = "a".repeat(40);
    let arguments = json!({
        "project": "agent:oe:webcodex",
        "expected_head": expected_head,
        "paths": ["src/tool_runtime/git.rs"],
        "message": private_message,
    });
    let raw = super::super::tool_audit::session_log_arguments_for_tool_request(
        "commit_git_paths",
        &arguments,
    );
    assert_eq!(raw["expected_head_valid"], true);
    assert_eq!(raw["expected_head"], "a".repeat(40));
    assert_eq!(raw["message_present"], true);
    assert_eq!(raw["paths"], json!(["src/tool_runtime/git.rs"]));
    assert!(!raw.to_string().contains(private_message));

    let call = ToolCall::from_tool_name("commit_git_paths", arguments).unwrap();
    let typed = call.session_log_arguments();
    assert_eq!(typed["expected_head_valid"], true);
    assert_eq!(typed["message_present"], true);
    assert!(!typed.to_string().contains(private_message));
}

#[test]
fn git_commit_marker_parser_keeps_mutation_evidence_strict() {
    let expected = "a".repeat(40);
    let new_head = "b".repeat(40);
    let valid = parse_git_commit_marker(&format!(
        "noise\n{GIT_COMMIT_RESULT_PREFIX} status=success previous={expected} new={new_head}\n"
    ))
    .unwrap();
    assert_eq!(valid.status, "success");
    assert_eq!(valid.previous_head.as_deref(), Some(expected.as_str()));
    assert_eq!(valid.new_head.as_deref(), Some(new_head.as_str()));

    let malformed = parse_git_commit_marker(&format!(
        "{GIT_COMMIT_RESULT_PREFIX} status=success previous={expected} new=not-a-sha\n"
    ))
    .unwrap();
    assert_eq!(malformed.status, "success");
    assert!(malformed.new_head.is_none());
}

#[tokio::test]
async fn git_restore_paths_restores_tracked_filename_containing_target() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(
        tmp.path(),
        "SMOKE_TARGET.txt",
        "original\n",
        "track smoke target",
    );
    fs::write(tmp.path().join("SMOKE_TARGET.txt"), "modified\n").unwrap();

    let runtime = test_runtime();
    let project = register_structured_git_agent_at_path(
        &runtime,
        "restore-target-substring",
        "repo",
        tmp.path(),
    )
    .await;
    let restore = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .git_restore_paths(project, vec!["SMOKE_TARGET.txt".to_string()])
                .await
        }
    });

    let request = wait_for_patch_agent_request(&runtime, "restore-target-substring").await;
    assert_eq!(request.kind, "run_process");
    assert!(request.command.is_empty());
    let process = request.process.as_ref().expect("typed git restore process");
    assert_eq!(process.executable, "git");
    assert_eq!(
        process.args,
        ["restore", "--", "SMOKE_TARGET.txt"].map(str::to_string)
    );
    complete_agent_request_by_running_locally(&runtime, "restore-target-substring", request).await;

    let result = restore.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(
        fs::read_to_string(tmp.path().join("SMOKE_TARGET.txt"))
            .unwrap()
            .replace("\r\n", "\n"),
        "original\n"
    );
    let (exit_code, stdout, stderr, _) = run_command_sync("git status --porcelain", tmp.path(), 30);
    assert_eq!(exit_code, 0, "git status failed: {stderr}");
    assert!(stdout.is_empty(), "worktree should be clean: {stdout}");
}

#[tokio::test]
async fn git_path_mutations_pass_shell_sensitive_paths_as_literal_argv() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let tracked = [
        "space name.txt",
        "quote'name.txt",
        "amp&semi;.txt",
        "dollar$(literal).txt",
    ];
    for path in tracked {
        commit_file(tmp.path(), path, "original\n", &format!("track {path}"));
        fs::write(tmp.path().join(path), "modified\n").unwrap();
    }

    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "literal-git-paths", "repo", tmp.path())
            .await;
    let restore_paths = tracked.map(str::to_string).to_vec();
    let restore = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let restore_paths = restore_paths.clone();
        async move { runtime.git_restore_paths(project, restore_paths).await }
    });
    let request = wait_for_patch_agent_request(&runtime, "literal-git-paths").await;
    assert_eq!(request.kind, "run_process");
    let process = request.process.as_ref().expect("typed git restore process");
    assert_eq!(process.executable, "git");
    let mut expected = vec!["restore".to_string(), "--".to_string()];
    expected.extend(restore_paths.iter().cloned());
    assert_eq!(process.args, expected);
    complete_agent_request_by_running_locally(&runtime, "literal-git-paths", request).await;
    assert!(restore.await.unwrap().success);
    for path in tracked {
        assert_eq!(
            fs::read_to_string(tmp.path().join(path))
                .unwrap()
                .replace("\r\n", "\n"),
            "original\n"
        );
    }

    let untracked = [
        "untracked space.txt",
        "untracked'quote.txt",
        "untracked&semi;.txt",
        "untracked$(literal).txt",
    ];
    for path in untracked {
        fs::write(tmp.path().join(path), "remove me\n").unwrap();
    }
    let discard_paths = untracked.map(str::to_string).to_vec();
    let discard = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let discard_paths = discard_paths.clone();
        async move { runtime.discard_untracked(project, discard_paths).await }
    });
    let request = wait_for_patch_agent_request(&runtime, "literal-git-paths").await;
    assert_eq!(request.kind, "run_process");
    let process = request.process.as_ref().expect("typed git clean process");
    assert_eq!(process.executable, "git");
    let mut expected = vec!["clean".to_string(), "-f".to_string(), "--".to_string()];
    expected.extend(discard_paths.iter().cloned());
    assert_eq!(process.args, expected);
    complete_agent_request_by_running_locally(&runtime, "literal-git-paths", request).await;
    assert!(discard.await.unwrap().success);
    for path in untracked {
        assert!(
            !tmp.path().join(path).exists(),
            "{path} should be removed literally"
        );
    }
}

#[tokio::test]
async fn git_restore_stays_sync_on_structured_job_capable_runner() {
    let runtime = runtime_with_agent_project("restore-sync-job-capable");
    register_agent(
        &runtime,
        "restore-sync-job-capable",
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            jobs: true,
            async_jobs: true,
            structured_process_argv: true,
            structured_execution_jobs: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("restore-sync-job-capable");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .git_restore_paths(project, vec!["safe.txt".to_string()])
                .await
        }
    });

    let request = wait_for_patch_agent_request(&runtime, "restore-sync-job-capable").await;
    assert_eq!(request.kind, "run_process");
    assert!(request.job_id.is_none());
    assert!(request.command.is_empty());
    let process = request.process.as_ref().expect("typed git restore process");
    assert_eq!(process.executable, "git");
    assert_eq!(
        process.args,
        ["restore", "--", "safe.txt"].map(str::to_string)
    );
    complete_patch_agent_request(
        &runtime,
        "restore-sync-job-capable",
        &request.request_id,
        0,
        "",
        "",
    )
    .await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["restored_paths"], json!(["safe.txt"]));
    assert!(
        probe_patch_agent_request(&runtime, "restore-sync-job-capable")
            .await
            .is_none()
    );
}

#[tokio::test]
async fn git_restore_replacement_after_dispatch_reports_outcome_unknown_without_retry() {
    let runtime = runtime_with_agent_project("restore-uncertain");
    register_agent(
        &runtime,
        "restore-uncertain",
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            structured_process_argv: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("restore-uncertain");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .git_restore_paths(project, vec!["safe.txt".to_string()])
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, "restore-uncertain").await;
    assert_eq!(request.kind, "run_process");

    runtime
        .runner_registry
        .set_last_seen_for_test("restore-uncertain", chrono::Utc::now().timestamp() - 120)
        .await;
    register_agent_with_instance(
        &runtime,
        "restore-uncertain",
        "inst-b",
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            structured_process_argv: true,
            ..Default::default()
        },
    )
    .await;

    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "outcome_unknown");
    assert_eq!(result.output["failure_kind"], "outcome_unknown");
    let retry = probe_agent_request_for_instance(&runtime, "restore-uncertain", "inst-b").await;
    assert!(
        retry.is_none(),
        "uncertain mutation must not be retried: {retry:?}"
    );
}


async fn collect_counted_review(
    runtime: &ToolRuntime, client: &str, task: tokio::task::JoinHandle<ToolResult>,
) -> (ToolResult, usize) {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut count = 0;
    while !task.is_finished() {
        assert!(tokio::time::Instant::now() < deadline, "review did not converge");
        if let Some(request) = probe_patch_agent_request(runtime, client).await {
            count += 1;
            complete_agent_request_by_running_locally(runtime, client, request).await;
        } else { tokio::time::sleep(std::time::Duration::from_millis(2)).await; }
    }
    (task.await.unwrap(), count)
}

fn full_workspace_review(runtime: &ToolRuntime, project: &str) -> tokio::task::JoinHandle<ToolResult> {
    let runtime = runtime.clone(); let project = project.to_owned();
    tokio::spawn(async move { runtime.review_changes(project,
        webcodex_tool_contracts::tool_call::GitReviewScopeInput::Workspace,
        None, None, None, None, None, None, None).await })
}

#[tokio::test]
async fn review_net_workspace_includes_staged_and_untracked_without_mutating_index() {
    let tmp = tempfile::tempdir().unwrap(); init_git_repo(tmp.path());
    commit_file(tmp.path(), "staged.txt", "before\n", "base");
    fs::write(tmp.path().join("staged.txt"), "after-staged\n").unwrap();
    git_test_command_ok(tmp.path(), "git add staged.txt");
    fs::write(tmp.path().join("new.txt"), "new-untracked\n").unwrap();
    let index = fs::read(tmp.path().join(".git/index")).unwrap();
    let runtime = test_runtime(); let client = "review-net-content";
    let project = register_structured_git_agent_at_path(&runtime, client, "repo", tmp.path()).await;
    let (result, count) = collect_counted_review(&runtime, client, full_workspace_review(&runtime, &project)).await;
    assert!(result.success, "{result:?}"); assert_eq!(count, 2);
    assert_eq!(result.output["diff"]["basis"], "head_to_frozen_workspace");
    let diff = result.output["diff"].to_string();
    assert!(diff.contains("+after-staged"), "{result:?}");
    assert!(diff.contains("+new-untracked"), "{result:?}");
    assert_eq!(result.output["summary"]["counts"]["staged"], 1);
    assert_eq!(index, fs::read(tmp.path().join(".git/index")).unwrap());
    assert!(result.output["diff"].get("recovery").is_none());
    assert!(result.output["continuation"].is_null());
    eprintln!("REVIEW_NET runner_observations=2 staged=true untracked=true index_unchanged=true");
}

#[tokio::test]
async fn review_unborn_and_staged_reversal_have_explicit_net_semantics() {
    let tmp = tempfile::tempdir().unwrap(); init_git_repo(tmp.path());
    fs::write(tmp.path().join("new.txt"), "unborn-content\n").unwrap();
    let runtime = test_runtime(); let client = "review-net-unborn";
    let project = register_structured_git_agent_at_path(&runtime, client, "repo", tmp.path()).await;
    let result = collect_review_task(&runtime, client, full_workspace_review(&runtime, &project)).await;
    assert!(result.success, "{result:?}");
    assert!(result.output["diff"].to_string().contains("+unborn-content"));
    assert!(result.output["snapshot"]["source"]["head_commit"].is_null());
    assert!(!tmp.path().join(".git/index").exists());
    git_test_command_ok(tmp.path(), "git add new.txt && git commit -m base");
    fs::write(tmp.path().join("new.txt"), "index-only-content\n").unwrap();
    git_test_command_ok(tmp.path(), "git add new.txt");
    fs::write(tmp.path().join("new.txt"), "unborn-content\n").unwrap();
    let result = collect_review_task(&runtime, client, full_workspace_review(&runtime, &project)).await;
    assert!(result.success, "{result:?}");
    assert_eq!(result.output["diff"]["hunk_count"], 0);
    assert_eq!(result.output["summary"]["counts"]["staged"], 1);
    assert_eq!(result.output["summary"]["counts"]["unstaged"], 1);
    assert_eq!(result.output["diff"]["basis"], "head_to_frozen_workspace");
}

#[tokio::test]
async fn review_default_one_line_edit_needs_no_fragment_and_continuations_use_one_hop() {
    let tmp = tempfile::tempdir().unwrap(); init_git_repo(tmp.path());
    let text = (0..400).map(|n| format!("line{n}\n")).collect::<String>();
    commit_file(tmp.path(), "large.txt", &text, "base");
    fs::write(tmp.path().join("large.txt"), text.replace("line200\n", "edited200\n")).unwrap();
    let runtime = test_runtime(); let client = "review-one-line";
    let project = register_structured_git_agent_at_path(&runtime, client, "repo", tmp.path()).await;
    let (result, count) = collect_counted_review(&runtime, client, full_workspace_review(&runtime, &project)).await;
    assert!(result.success, "{result:?}"); assert_eq!(count, 2);
    assert_eq!(result.output["diff"]["truncated"], false);
    assert!(result.output["continuation"].is_null());
    assert!(result.output["diff"].to_string().len() < 2000);
    let tmp = review_paging_repo(); let client = "review-hops";
    let project = register_structured_git_agent_at_path(&runtime, client, "repo", tmp.path()).await;
    let mut next = None;
    for page in 0..3 {
        let (result, count) = collect_counted_review(&runtime, client, start_review_page(&runtime, &project, next)).await;
        assert!(result.success, "{result:?}"); assert_eq!(count, if page == 0 { 2 } else { 1 });
        next = result.output["continuation"].as_str().map(str::to_owned);
        assert_eq!(result.output["diff"]["has_more"], next.is_some());
        assert!(result.output["diff"].get("recovery").is_none());
    }
    assert!(next.is_none());
    eprintln!("REVIEW_PAGES default_one_line_fragments=0 first_hops=2 continuation_hops=1");
}


#[tokio::test]
async fn review_committed_scope_is_resolved_once_and_survives_live_workspace_changes() {
    let tmp = review_paging_repo();
    let hash = |root: &std::path::Path| {
        let output = std::process::Command::new("git").args(["rev-parse", "HEAD"])
            .current_dir(root).output().unwrap();
        assert!(output.status.success()); String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    let base = hash(tmp.path());
    git_test_command_ok(tmp.path(), "git add . && git commit -m reviewed");
    let head = hash(tmp.path());
    let runtime = test_runtime(); let client = "review-committed-hops";
    let project = register_structured_git_agent_at_path(&runtime, client, "repo", tmp.path()).await;
    let mut next = None;
    for page in 0..3 {
        let task = tokio::spawn({
            let runtime = runtime.clone(); let project = project.clone();
            let (base_commit, head_commit) = (base.clone(), head.clone()); let continuation = next.take();
            async move { runtime.review_changes(project,
                webcodex_tool_contracts::tool_call::GitReviewScopeInput::Committed { base_commit, head_commit },
                None,None,Some(1),None,None,continuation,None).await }
        });
        let (result, calls) = collect_counted_review(&runtime, client, task).await;
        assert!(result.success, "{result:?}");
        if page == 0 { assert!((3..=4).contains(&calls)); } else { assert_eq!(calls,1); }
        assert_eq!(result.output["snapshot"]["source"]["merge_base"], base);
        assert_eq!(result.output["snapshot"]["source"]["requested_head"], head);
        next = result.output["continuation"].as_str().map(str::to_owned);
        assert!(result.output["diff"].get("recovery").is_none());
        fs::write(tmp.path().join("a.txt"), format!("unreviewed live change {page}\n")).unwrap();
    }
    assert!(next.is_none());
    eprintln!("REVIEW_COMMITTED scope_resolutions=1 continuation_hops=1 mutable_worktree_ignored=true");
}

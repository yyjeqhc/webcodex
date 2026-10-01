#[test]
fn git_read_commands_are_non_mutating_and_log_is_bounded() {
    assert_eq!(normalize_git_log_limit(None), 20);
    assert_eq!(normalize_git_log_limit(Some(0)), 20);
    assert_eq!(normalize_git_log_limit(Some(999)), 100);
    assert_eq!(normalize_git_log_skip(Some(20_000)), 10_000);

    let log = git_log_command(21, 7);
    assert!(log.contains("git log"));
    assert!(log.contains("-n 22"));
    assert!(log.contains("--skip 7"));

    for forbidden in [
        "git apply",
        "git commit",
        "git checkout",
        "git reset",
        "git push",
        "git stash",
        "git merge",
        "git rebase",
        "git rm ",
    ] {
        assert!(
            !log.contains(forbidden),
            "read_git_log command must not contain {forbidden:?}: {log}"
        );
    }
}

#[test]
fn git_log_parser_splits_commits_refs_and_truncation() {
    let stdout = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\u{1f}aaaaaaa\u{1f}HEAD -> main, tag: v1\u{1f}Ada\u{1f}ada@example.com\u{1f}2026-06-30T00:00:00+00:00\u{1f}newest\u{1e}bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\u{1f}bbbbbbb\u{1f}\u{1f}Ben\u{1f}ben@example.com\u{1f}2026-06-29T00:00:00+00:00\u{1f}older\u{1e}";
    let (commits, truncated) = parse_git_log_commits(stdout, 1).unwrap();
    assert!(truncated);
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0]["short_hash"], "aaaaaaa");
    assert_eq!(commits[0]["subject"], "newest");
    assert_eq!(commits[0]["refs"], json!(["HEAD", "main", "v1"]));

    for marker in [
        "[output truncated]\n",
        "[...]\n",
        "[output truncated to last 262144 bytes]\n",
    ] {
        assert!(parse_git_log_commits(&format!("{marker}{stdout}"), 1).is_err());
    }
    assert!(parse_git_log_commits(stdout.trim_end_matches('\u{1e}'), 1).is_err());
    assert!(parse_git_log_commits("partial record\u{1e}", 1).is_err());
}

fn write_git_review_fixture_file(root: &Path, path: &str, content: &str) {
    let full = root.join(path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(full, content).unwrap();
}

fn commit_git_review_fixture(root: &Path, subject: &str) -> String {
    for cmd in [
        "git add -A".to_string(),
        format!("git commit -m {}", shell_escape_simple(subject)),
    ] {
        let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, root, 30);
        assert_eq!(
            exit_code, 0,
            "git review fixture command failed: {cmd}\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }
    let (exit_code, stdout, stderr, _) = run_command_sync("git rev-parse HEAD", root, 30);
    assert_eq!(exit_code, 0, "rev-parse failed: {stderr}");
    let sha = stdout.trim().to_string();
    assert_eq!(sha.len(), 40);
    sha
}

async fn run_git_review_summary_via_agent(
    runtime: &ToolRuntime,
    client_id: &str,
    project: String,
    base_commit: String,
    head_commit: String,
) -> ToolResult {
    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .git_review_summary(project, base_commit, head_commit)
            .await
    });
    let timeout_secs = if cfg!(windows) { 60 } else { 10 };
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    while !task.is_finished() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "read_git_review_summary did not finish within {timeout_secs} seconds for client {client_id}"
        );
        if let Some(request) = probe_patch_agent_request(runtime, client_id).await {
            assert_eq!(request.kind, "run_internal_posix_script");
            assert!(request.command.is_empty());
            let payload = request
                .script
                .as_ref()
                .expect("read_git_review_summary must use a typed internal script");
            assert_eq!(
                payload.language,
                crate::runner_protocol::ShellScriptLanguage::Sh
            );
            assert!(payload.args.is_empty());
            assert!(payload.script.contains("GIT_NO_REPLACE_OBJECTS=1"));
            assert!(payload.script.contains("GIT_NO_LAZY_FETCH=1"));
            assert!(payload.script.contains("GIT_ATTR_NOSYSTEM=1"));
            assert!(payload.script.contains("attributesFile = /dev/null"));
            assert!(payload.script.contains("GIT_CONFIG_GLOBAL=/dev/null"));
            assert!(payload.script.contains("git read-tree "));
            assert!(payload
                .script
                .contains("git ls-files -z -- .gitattributes ':(glob)**/.gitattributes'"));
            assert!(payload
                .script
                .contains("git checkout-index -z --stdin --prefix=\"$view/worktree/\""));
            assert!(!payload.script.contains("checkout-index -a"));
            assert!(!payload.script.contains("checkout-index --all"));
            for forbidden in [
                "git fetch",
                "git apply",
                "git commit",
                "git checkout ",
                "git reset",
                "git push",
                "git stash",
                "git merge ",
                "git rebase",
                "git clean",
                "git add ",
            ] {
                assert!(
                    !payload.script.contains(forbidden),
                    "read_git_review_summary internal script must remain read-only; found {forbidden}: {}",
                    payload.script
                );
            }
            if payload.script.contains(" diff ") {
                assert!(payload.script.contains("--no-ext-diff"));
                assert!(payload.script.contains("--no-textconv"));
            }
            let (exit_code, stdout, stderr) = run_runner_shell_request_locally(&request);
            assert_eq!(
                exit_code, 0,
                "read_git_review_summary internal script failed\nscript:\n{}\nstdout:\n{}\nstderr:\n{}",
                payload.script, stdout, stderr
            );
            complete_patch_agent_request(
                runtime,
                client_id,
                &request.request_id,
                exit_code,
                &stdout,
                &stderr,
            )
            .await;
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
    task.await.unwrap()
}

#[tokio::test]
async fn git_review_summary_maps_exact_committed_range_without_raw_diff() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    write_git_review_fixture_file(tmp.path(), ".gitattributes", "*.rs diff=rust\n");
    write_git_review_fixture_file(
        tmp.path(),
        "src/auth/scopes.rs",
        "pub fn auth_scope() -> bool {\n    false\n}\n",
    );
    write_git_review_fixture_file(
        tmp.path(),
        "src/protocol.rs",
        "pub fn wire_version() -> u8 {\n    1\n}\n",
    );
    write_git_review_fixture_file(
        tmp.path(),
        "src/runtime/job.rs",
        "pub fn job_state() -> &'static str {\n    \"RAW_SECRET_BODY_MARKER_OLD\"\n}\n",
    );
    write_git_review_fixture_file(
        tmp.path(),
        "src/tokenizer.rs",
        "pub fn tokenizer_mode() -> u8 {\n    1\n}\n",
    );
    write_git_review_fixture_file(tmp.path(), "tests/auth.rs", "assert_eq!(1, 1);\n");
    write_git_review_fixture_file(tmp.path(), "docs/AUTH.md", "old auth docs\n");
    let base = commit_git_review_fixture(tmp.path(), "base");

    write_git_review_fixture_file(
        tmp.path(),
        "src/auth/scopes.rs",
        "pub fn auth_scope() -> bool {\n    true\n}\n",
    );
    write_git_review_fixture_file(
        tmp.path(),
        "src/protocol.rs",
        "pub fn wire_version() -> u8 {\n    2\n}\n",
    );
    write_git_review_fixture_file(
        tmp.path(),
        "src/runtime/job.rs",
        "pub fn job_state() -> &'static str {\n    \"RAW_SECRET_BODY_MARKER_NEW\"\n}\n",
    );
    write_git_review_fixture_file(
        tmp.path(),
        "src/tokenizer.rs",
        "pub fn tokenizer_mode() -> u8 {\n    2\n}\n",
    );
    write_git_review_fixture_file(tmp.path(), "tests/auth.rs", "assert_eq!(2, 2);\n");
    write_git_review_fixture_file(tmp.path(), "docs/AUTH.md", "new auth docs\n");
    let head = commit_git_review_fixture(tmp.path(), "head");

    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "review-summary", "repo", tmp.path()).await;
    let result = run_git_review_summary_via_agent(
        &runtime,
        "review-summary",
        project,
        base.clone(),
        head.clone(),
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    let output = &result.output;
    assert_eq!(output["scope"]["requested_base"], base);
    assert_eq!(output["scope"]["requested_head"], head);
    assert_eq!(
        output["scope"]["merge_base"],
        output["scope"]["requested_base"]
    );
    assert_eq!(output["scope"]["base_is_ancestor"], true);
    assert_eq!(output["scope"]["commit_count"], 1);
    assert_eq!(output["stats"]["files_changed"], 6);
    assert_eq!(output["stats"]["insertions"], 6);
    assert_eq!(output["stats"]["deletions"], 6);
    assert_eq!(output["stats"]["binary_files"], 0);
    assert_eq!(output["coverage"]["production_changed"], true);
    assert_eq!(output["coverage"]["tests_changed"], true);
    assert_eq!(output["coverage"]["docs_changed"], true);
    assert_eq!(output["deterministic"], true);
    assert_eq!(output["llm_summary"], false);
    assert_eq!(output["truncated"], false, "{output}");

    let signals = output["signals"].as_array().unwrap();
    let signal_names = signals
        .iter()
        .filter_map(|signal| signal["name"].as_str())
        .collect::<HashSet<_>>();
    for expected in [
        "auth_or_scope_surface_touched",
        "protocol_or_wire_schema_surface_touched",
        "execution_lifecycle_surface_touched",
    ] {
        assert!(
            signal_names.contains(expected),
            "missing {expected}: {signals:?}"
        );
    }
    assert!(!signal_names.contains("production_without_test_changes"));
    assert!(!signal_names.contains("contract_surface_without_doc_changes"));

    let files = output["files"].as_array().unwrap();
    assert_eq!(files.len(), 6);
    let tokenizer = files
        .iter()
        .find(|file| file["path"] == "src/tokenizer.rs")
        .unwrap();
    assert!(tokenizer["classes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|class| class == "production"));
    assert!(!tokenizer["classes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|class| class == "auth_security"));
    let auth = files
        .iter()
        .find(|file| file["path"] == "src/auth/scopes.rs")
        .unwrap();
    assert!(auth["classes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|class| class == "auth_security"));
    assert!(auth["symbols"].as_array().unwrap().len() <= GIT_REVIEW_MAX_SYMBOLS_PER_FILE);
    assert!(
        output["truncation"]["symbols_returned"].as_u64().unwrap()
            <= GIT_REVIEW_MAX_TOTAL_SYMBOLS as u64
    );

    let serialized = serde_json::to_string(output).unwrap();
    assert!(!serialized.contains("RAW_SECRET_BODY_MARKER_OLD"));
    assert!(!serialized.contains("RAW_SECRET_BODY_MARKER_NEW"));
    assert!(!serialized.contains("@@ -"));

    let audit = crate::tool_runtime::audit_safe_result_for_tool("read_git_review_summary", output);
    assert!(
        audit.get("files").is_none(),
        "audit must not persist paths/symbols: {audit}"
    );
    assert!(
        audit.get("signals").is_none(),
        "audit must not persist signal paths: {audit}"
    );
    assert_eq!(
        audit["scope"]["requested_base"],
        output["scope"]["requested_base"]
    );
    assert_eq!(audit["stats"], output["stats"]);
}

#[tokio::test]
async fn git_review_summary_exact_range_ignores_mutable_git_attributes_and_config() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "data.txt", "a\nb\n", "base");
    let base = {
        let (_, stdout, _, _) = run_command_sync("git rev-parse HEAD", tmp.path(), 30);
        stdout.trim().to_string()
    };
    commit_file(tmp.path(), "data.txt", "a\nc\n", "head");
    let head = {
        let (_, stdout, _, _) = run_command_sync("git rev-parse HEAD", tmp.path(), 30);
        stdout.trim().to_string()
    };

    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "review-attributes", "repo", tmp.path())
            .await;
    let before = run_git_review_summary_via_agent(
        &runtime,
        "review-attributes",
        project.clone(),
        base.clone(),
        head.clone(),
    )
    .await;
    assert!(before.success, "{:?}", before.error);
    assert_eq!(before.output["stats"]["files_changed"], 1);
    assert_eq!(before.output["stats"]["insertions"], 1);
    assert_eq!(before.output["stats"]["deletions"], 1);
    assert_eq!(before.output["stats"]["binary_files"], 0);

    fs::write(tmp.path().join(".gitattributes"), "data.txt -diff\n").unwrap();
    fs::create_dir_all(tmp.path().join(".git/info")).unwrap();
    fs::write(tmp.path().join(".git/info/attributes"), "data.txt -diff\n").unwrap();
    let mutable_attributes = tmp.path().join("mutable.attributes");
    fs::write(&mutable_attributes, "data.txt -diff\n").unwrap();
    let config_command = format!(
        "git config core.attributesFile {}",
        shell_escape_simple(mutable_attributes.to_string_lossy().as_ref())
    );
    let (exit_code, _, stderr, _) = run_command_sync(&config_command, tmp.path(), 30);
    assert_eq!(exit_code, 0, "{stderr}");

    let after =
        run_git_review_summary_via_agent(&runtime, "review-attributes", project, base, head).await;
    assert!(after.success, "{:?}", after.error);
    assert_eq!(
        after.output, before.output,
        "mutable worktree/info/config attributes must not change an exact committed review"
    );
}

#[tokio::test]
async fn git_review_summary_uses_reviewed_head_committed_attributes() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "data.txt", "a\nb\n", "base");
    let base = {
        let (_, stdout, _, _) = run_command_sync("git rev-parse HEAD", tmp.path(), 30);
        stdout.trim().to_string()
    };
    write_git_review_fixture_file(tmp.path(), ".gitattributes", "data.txt -diff\n");
    write_git_review_fixture_file(tmp.path(), "data.txt", "a\nc\n");
    let head = commit_git_review_fixture(tmp.path(), "head attributes");

    let runtime = test_runtime();
    let project = register_structured_git_agent_at_path(
        &runtime,
        "review-head-attributes",
        "repo",
        tmp.path(),
    )
    .await;
    let result =
        run_git_review_summary_via_agent(&runtime, "review-head-attributes", project, base, head)
            .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["stats"]["files_changed"], 2);
    assert_eq!(result.output["stats"]["binary_files"], 1);
    let data = result.output["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == "data.txt")
        .unwrap();
    assert_eq!(data["binary"], true);
    assert_eq!(data["symbol_inspection"], "skipped_binary");
}

#[tokio::test]
async fn git_review_summary_uses_nested_reviewed_head_attributes_without_mutable_leakage() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    write_git_review_fixture_file(tmp.path(), "src/nested/data.txt", "a\nb\n");
    let base = commit_git_review_fixture(tmp.path(), "nested base");

    write_git_review_fixture_file(tmp.path(), "src/nested/.gitattributes", "data.txt -diff\n");
    write_git_review_fixture_file(tmp.path(), "src/nested/data.txt", "a\nc\n");
    let head = commit_git_review_fixture(tmp.path(), "nested head attributes");

    write_git_review_fixture_file(tmp.path(), "src/nested/.gitattributes", "data.txt diff\n");
    fs::create_dir_all(tmp.path().join(".git/info")).unwrap();
    fs::write(
        tmp.path().join(".git/info/attributes"),
        "src/nested/data.txt diff\n",
    )
    .unwrap();

    let runtime = test_runtime();
    let project = register_structured_git_agent_at_path(
        &runtime,
        "review-nested-head-attributes",
        "repo",
        tmp.path(),
    )
    .await;
    let result = run_git_review_summary_via_agent(
        &runtime,
        "review-nested-head-attributes",
        project,
        base,
        head,
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["stats"]["files_changed"], 2);
    assert_eq!(result.output["stats"]["binary_files"], 1);
    let data = result.output["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == "src/nested/data.txt")
        .unwrap();
    assert_eq!(data["binary"], true);
    assert_eq!(data["symbol_inspection"], "skipped_binary");
}

#[tokio::test]
async fn git_review_summary_exact_range_prefers_reviewed_head_over_dirty_and_info_attributes() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    write_git_review_fixture_file(tmp.path(), "data.txt", "a\nb\n");
    let base = commit_git_review_fixture(tmp.path(), "root base");

    write_git_review_fixture_file(tmp.path(), ".gitattributes", "data.txt -diff\n");
    write_git_review_fixture_file(tmp.path(), "data.txt", "a\nc\n");
    let head = commit_git_review_fixture(tmp.path(), "root head attributes");

    write_git_review_fixture_file(tmp.path(), ".gitattributes", "data.txt diff\n");
    fs::create_dir_all(tmp.path().join(".git/info")).unwrap();
    fs::write(tmp.path().join(".git/info/attributes"), "data.txt diff\n").unwrap();

    let runtime = test_runtime();
    let project = register_structured_git_agent_at_path(
        &runtime,
        "review-root-head-attributes",
        "repo",
        tmp.path(),
    )
    .await;
    let result = run_git_review_summary_via_agent(
        &runtime,
        "review-root-head-attributes",
        project,
        base,
        head,
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["stats"]["files_changed"], 2);
    assert_eq!(result.output["stats"]["binary_files"], 1);
    let data = result.output["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == "data.txt")
        .unwrap();
    assert_eq!(data["binary"], true);
    assert_eq!(data["symbol_inspection"], "skipped_binary");
}

#[tokio::test]
async fn git_review_summary_rename_preserves_old_path_classification_and_privacy() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    write_git_review_fixture_file(
        tmp.path(),
        "src/auth/scopes.rs",
        "pub fn auth_scope() -> bool { true }\n",
    );
    write_git_review_fixture_file(
        tmp.path(),
        ".env",
        concat!(
            "pub fn credential_fixture() -> u8 {\n",
            "    let a = 1;\n",
            "    let b = 2;\n",
            "    let c = 3;\n",
            "    let d = 4;\n",
            "    let e = 5;\n",
            "    a + b + c + d + e + 1\n",
            "}\n",
        ),
    );
    let base = commit_git_review_fixture(tmp.path(), "rename base");

    fs::rename(
        tmp.path().join("src/auth/scopes.rs"),
        tmp.path().join("src/tokenizer.rs"),
    )
    .unwrap();
    fs::rename(tmp.path().join(".env"), tmp.path().join("src/config.rs")).unwrap();
    write_git_review_fixture_file(
        tmp.path(),
        "src/config.rs",
        concat!(
            "pub fn credential_fixture() -> u8 {\n",
            "    let a = 1;\n",
            "    let b = 2;\n",
            "    let c = 3;\n",
            "    let d = 4;\n",
            "    let e = 5;\n",
            "    a + b + c + d + e + 2\n",
            "}\n",
        ),
    );
    let head = commit_git_review_fixture(tmp.path(), "rename head");

    let runtime = test_runtime();
    let project = register_structured_git_agent_at_path(
        &runtime,
        "review-rename-boundaries",
        "repo",
        tmp.path(),
    )
    .await;
    let result =
        run_git_review_summary_via_agent(&runtime, "review-rename-boundaries", project, base, head)
            .await;
    assert!(result.success, "{:?}", result.error);
    let files = result.output["files"].as_array().unwrap();

    let tokenizer = files
        .iter()
        .find(|file| file["path"] == "src/tokenizer.rs")
        .unwrap();
    assert_eq!(tokenizer["status"], "renamed");
    assert_eq!(tokenizer["previous_path"], "src/auth/scopes.rs");
    assert!(tokenizer["classes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|class| class == "auth_security"));
    assert!(result.output["signals"]
        .as_array()
        .unwrap()
        .iter()
        .any(|signal| signal["name"] == "auth_or_scope_surface_touched"));

    let credential = files
        .iter()
        .find(|file| file["path"] == "src/config.rs")
        .unwrap();
    assert_eq!(credential["status"], "renamed");
    assert_eq!(credential["previous_path"], ".env");
    assert_eq!(
        credential["symbol_inspection"],
        "skipped_sensitive_or_excluded"
    );
    assert_eq!(credential["symbols"], json!([]));
}

#[tokio::test]
async fn git_review_summary_uses_merge_base_when_requested_base_is_not_ancestor() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "base.txt", "root\n", "root");
    let root = {
        let (_, stdout, _, _) = run_command_sync("git rev-parse HEAD", tmp.path(), 30);
        stdout.trim().to_string()
    };
    let root_branch = {
        let (_, stdout, _, _) = run_command_sync("git branch --show-current", tmp.path(), 30);
        stdout.trim().to_string()
    };
    assert!(!root_branch.is_empty());
    let (exit_code, _, stderr, _) = run_command_sync("git checkout -b feature", tmp.path(), 30);
    assert_eq!(exit_code, 0, "{stderr}");
    commit_file(tmp.path(), "feature.txt", "feature\n", "feature");
    let head = {
        let (_, stdout, _, _) = run_command_sync("git rev-parse HEAD", tmp.path(), 30);
        stdout.trim().to_string()
    };
    let checkout_root = format!("git checkout {}", shell_escape_simple(&root_branch));
    let (exit_code, _, stderr, _) = run_command_sync(&checkout_root, tmp.path(), 30);
    assert_eq!(exit_code, 0, "{stderr}");
    commit_file(tmp.path(), "main.txt", "main\n", "main");
    let requested_base = {
        let (_, stdout, _, _) = run_command_sync("git rev-parse HEAD", tmp.path(), 30);
        stdout.trim().to_string()
    };

    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "review-merge-base", "repo", tmp.path())
            .await;
    let result = run_git_review_summary_via_agent(
        &runtime,
        "review-merge-base",
        project,
        requested_base.clone(),
        head.clone(),
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["scope"]["requested_base"], requested_base);
    assert_eq!(result.output["scope"]["requested_head"], head);
    assert_eq!(result.output["scope"]["merge_base"], root);
    assert_eq!(result.output["scope"]["base_is_ancestor"], false);
    assert_eq!(result.output["scope"]["commit_count"], 1);
    assert_eq!(result.output["stats"]["files_changed"], 1);
    assert_eq!(result.output["files"][0]["path"], "feature.txt");
}

#[tokio::test]
async fn git_review_summary_no_change_and_missing_object_are_structured() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "same\n", "root");
    let exact = {
        let (_, stdout, _, _) = run_command_sync("git rev-parse HEAD", tmp.path(), 30);
        stdout.trim().to_string()
    };
    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "review-empty", "repo", tmp.path()).await;
    let empty = run_git_review_summary_via_agent(
        &runtime,
        "review-empty",
        project.clone(),
        exact.clone(),
        exact.clone(),
    )
    .await;
    assert!(empty.success, "{:?}", empty.error);
    assert_eq!(empty.output["stats"]["files_changed"], 0);
    assert_eq!(empty.output["files"], json!([]));
    assert_eq!(empty.output["coverage"]["production_changed"], false);
    assert_eq!(empty.output["truncated"], false);

    let missing = "f".repeat(40);
    let failed =
        run_git_review_summary_via_agent(&runtime, "review-empty", project, exact, missing).await;
    assert!(!failed.success);
    assert_eq!(
        failed.output["reason_code"],
        "head_commit_missing_or_not_commit"
    );
}

#[tokio::test]
async fn git_review_summary_real_git_edges_cover_rename_delete_add_binary_and_utf8() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    write_git_review_fixture_file(
        tmp.path(),
        "old name.rs",
        "pub fn renamed() -> u8 {\n    1\n}\n",
    );
    write_git_review_fixture_file(tmp.path(), "deleted.rs", "pub fn deleted() {}\n");
    write_git_review_fixture_file(tmp.path(), "路径.rs", "pub fn utf8() -> u8 {\n    1\n}\n");
    fs::write(tmp.path().join("binary.bin"), [0u8, 1, 0, 2, 3]).unwrap();
    let base = commit_git_review_fixture(tmp.path(), "edge base");

    fs::rename(
        tmp.path().join("old name.rs"),
        tmp.path().join("new name.rs"),
    )
    .unwrap();
    fs::remove_file(tmp.path().join("deleted.rs")).unwrap();
    write_git_review_fixture_file(tmp.path(), "added.rs", "pub fn added() {}\n");
    write_git_review_fixture_file(tmp.path(), "路径.rs", "pub fn utf8() -> u8 {\n    2\n}\n");
    fs::write(tmp.path().join("binary.bin"), [0u8, 9, 0, 8, 7]).unwrap();
    let head = commit_git_review_fixture(tmp.path(), "edge head");

    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "review-edges", "repo", tmp.path()).await;
    let result =
        run_git_review_summary_via_agent(&runtime, "review-edges", project, base, head).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["stats"]["files_changed"], 5);
    assert_eq!(result.output["stats"]["binary_files"], 1);
    let files = result.output["files"].as_array().unwrap();
    let renamed = files
        .iter()
        .find(|file| file["path"] == "new name.rs")
        .unwrap();
    assert_eq!(renamed["status"], "renamed");
    assert_eq!(renamed["previous_path"], "old name.rs");
    let deleted = files
        .iter()
        .find(|file| file["path"] == "deleted.rs")
        .unwrap();
    assert_eq!(deleted["status"], "deleted");
    let added = files
        .iter()
        .find(|file| file["path"] == "added.rs")
        .unwrap();
    assert_eq!(added["status"], "added");
    assert!(files.iter().any(|file| file["path"] == "路径.rs"));
    let binary = files
        .iter()
        .find(|file| file["path"] == "binary.bin")
        .unwrap();
    assert_eq!(binary["binary"], true);
    assert_eq!(binary["symbol_inspection"], "skipped_binary");
    assert_eq!(binary["symbols"], json!([]));
}

#[tokio::test]
async fn git_review_summary_large_file_count_reports_partial_coverage_truthfully() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "base\n", "base");
    let base = {
        let (_, stdout, _, _) = run_command_sync("git rev-parse HEAD", tmp.path(), 30);
        stdout.trim().to_string()
    };
    for index in 0..(GIT_REVIEW_MAX_FILES + 2) {
        write_git_review_fixture_file(
            tmp.path(),
            &format!("src/generated/file_{index:03}.rs"),
            &format!("pub fn generated_{index:03}() -> usize {{ {index} }}\n"),
        );
    }
    let head = commit_git_review_fixture(tmp.path(), "many files");
    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "review-many", "repo", tmp.path()).await;
    let result =
        run_git_review_summary_via_agent(&runtime, "review-many", project, base, head).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(
        result.output["stats"]["files_changed"],
        (GIT_REVIEW_MAX_FILES + 2) as u64
    );
    assert_eq!(
        result.output["truncation"]["files_returned"],
        GIT_REVIEW_MAX_FILES as u64
    );
    assert_eq!(result.output["truncation"]["files_truncated"], true);
    assert_eq!(result.output["coverage"]["production_changed"], true);
    assert!(result.output["coverage"]["tests_changed"].is_null());
    assert!(result.output["coverage"]["docs_changed"].is_null());
    assert_eq!(result.output["coverage"]["partial"], true);
    assert_eq!(result.output["truncated"], true);
}

#[tokio::test]
async fn git_review_summary_large_diff_saturates_symbol_probe_without_source_output() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let old_body = format!(
        "pub fn huge() -> &'static str {{\n    \"{}\"\n}}\n",
        "A".repeat(GIT_REVIEW_MAX_DIFF_BYTES + 4096)
    );
    write_git_review_fixture_file(tmp.path(), "src/runtime/huge.rs", &old_body);
    let base = commit_git_review_fixture(tmp.path(), "huge base");
    let new_body = format!(
        "pub fn huge() -> &'static str {{\n    \"{}\"\n}}\n",
        "B".repeat(GIT_REVIEW_MAX_DIFF_BYTES + 4096)
    );
    write_git_review_fixture_file(tmp.path(), "src/runtime/huge.rs", &new_body);
    let head = commit_git_review_fixture(tmp.path(), "huge head");
    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "review-huge", "repo", tmp.path()).await;
    let result =
        run_git_review_summary_via_agent(&runtime, "review-huge", project, base, head).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["truncation"]["symbols_partial"], true);
    assert_eq!(result.output["truncated"], true);
    assert!(
        result.output["truncation"]["diff_bytes_inspected"]
            .as_u64()
            .unwrap()
            <= GIT_REVIEW_MAX_DIFF_BYTES as u64
    );
    let serialized = serde_json::to_string(&result.output).unwrap();
    assert!(!serialized.contains(&"A".repeat(256)));
    assert!(!serialized.contains(&"B".repeat(256)));
}

#[tokio::test]
async fn git_review_summary_structures_non_git_no_merge_base_and_unborn_head() {
    let non_git = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "review-non-git", "repo", non_git.path())
            .await;
    let failed = run_git_review_summary_via_agent(
        &runtime,
        "review-non-git",
        project,
        "a".repeat(40),
        "b".repeat(40),
    )
    .await;
    assert!(!failed.success);
    assert_eq!(failed.output["reason_code"], "not_a_git_repository");

    let disconnected = tempfile::tempdir().unwrap();
    init_git_repo(disconnected.path());
    commit_file(disconnected.path(), "one.txt", "one\n", "one");
    let first = {
        let (_, stdout, _, _) = run_command_sync("git rev-parse HEAD", disconnected.path(), 30);
        stdout.trim().to_string()
    };
    let (exit_code, _, stderr, _) = run_command_sync(
        "git checkout --orphan disconnected",
        disconnected.path(),
        30,
    );
    assert_eq!(exit_code, 0, "{stderr}");
    let (exit_code, _, stderr, _) = run_command_sync("git rm -rf .", disconnected.path(), 30);
    assert_eq!(exit_code, 0, "{stderr}");
    commit_file(disconnected.path(), "two.txt", "two\n", "two");
    let second = {
        let (_, stdout, _, _) = run_command_sync("git rev-parse HEAD", disconnected.path(), 30);
        stdout.trim().to_string()
    };
    let runtime = test_runtime();
    let project = register_structured_git_agent_at_path(
        &runtime,
        "review-disconnected",
        "repo",
        disconnected.path(),
    )
    .await;
    let failed =
        run_git_review_summary_via_agent(&runtime, "review-disconnected", project, first, second)
            .await;
    assert!(!failed.success);
    assert_eq!(failed.output["reason_code"], "no_merge_base");

    let unborn = tempfile::tempdir().unwrap();
    init_git_repo(unborn.path());
    commit_file(unborn.path(), "kept.txt", "kept\n", "kept");
    let exact = {
        let (_, stdout, _, _) = run_command_sync("git rev-parse HEAD", unborn.path(), 30);
        stdout.trim().to_string()
    };
    let (exit_code, _, stderr, _) = run_command_sync(
        "git symbolic-ref HEAD refs/heads/unborn-review",
        unborn.path(),
        30,
    );
    assert_eq!(exit_code, 0, "{stderr}");
    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "review-unborn", "repo", unborn.path())
            .await;
    let result = run_git_review_summary_via_agent(
        &runtime,
        "review-unborn",
        project,
        exact.clone(),
        exact.clone(),
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["scope"]["requested_head"], exact);
    assert_eq!(result.output["stats"]["files_changed"], 0);
}

#[test]
fn git_review_summary_tool_schema_metadata_and_oauth_are_read_only() {
    use crate::auth::scopes::{oauth_scope_policy_for_runtime_tool, OAuthToolScopePolicy};
    use crate::auth::SCOPE_PROJECT_READ;
    use crate::tool_runtime::metadata::{lookup_tool_metadata, ToolRisk};
    let base = "a".repeat(40);
    let head = "b".repeat(40);
    let call = ToolCall::from_tool_name(
        "read_git_review_summary",
        json!({
            "project": SAMPLE_PROJECT,
            "base_commit": base,
            "head_commit": head,
            "session_id": "wc_sess_review"
        }),
    )
    .expect("read_git_review_summary should parse through generic ToolCall");
    match call {
        ToolCall::GitReviewSummary {
            project,
            base_commit,
            head_commit,
            session_id,
        } => {
            assert_eq!(project, SAMPLE_PROJECT);
            assert_eq!(base_commit, "a".repeat(40));
            assert_eq!(head_commit, "b".repeat(40));
            assert_eq!(session_id.as_deref(), Some("wc_sess_review"));
        }
        other => panic!("expected read_git_review_summary, got {other:?}"),
    }
    let malformed_request_audit =
        crate::tool_runtime::tool_audit::session_log_arguments_for_tool_request(
            "read_git_review_summary",
            &json!({
                "project": SAMPLE_PROJECT,
                "base_commit": "a".repeat(40),
                "head_commit": "b".repeat(40),
                "source_body": "must-not-persist"
            }),
        );
    assert_eq!(malformed_request_audit, json!({}));
    let request_audit = ToolCall::GitReviewSummary {
        project: SAMPLE_PROJECT.to_string(),
        base_commit: "a".repeat(40),
        head_commit: "b".repeat(40),
        session_id: None,
    }
    .session_log_arguments();
    assert_eq!(request_audit["project"], SAMPLE_PROJECT);
    assert_eq!(request_audit["base_commit"], "a".repeat(40));
    assert_eq!(request_audit["head_commit"], "b".repeat(40));
    assert_eq!(request_audit["base_commit_valid"], true);
    assert_eq!(request_audit["head_commit_valid"], true);
    let malformed = "source-like-invalid-value".repeat(1024);
    let malformed_audit = crate::tool_runtime::tool_audit::session_log_arguments_for_tool_request(
        "read_git_review_summary",
        &json!({
            "project": SAMPLE_PROJECT,
            "base_commit": malformed,
            "head_commit": "b".repeat(40)
        }),
    );
    assert_eq!(malformed_audit["base_commit_valid"], false);
    assert!(malformed_audit.get("base_commit").is_none());
    assert_eq!(malformed_audit["head_commit_valid"], true);
    let audit_text = serde_json::to_string(&malformed_audit).unwrap();
    assert!(!audit_text.contains("source-like-invalid-value"));

    let typed_malformed = ToolCall::GitReviewSummary {
        project: SAMPLE_PROJECT.to_string(),
        base_commit: "source-like-invalid-value".repeat(1024),
        head_commit: "b".repeat(40),
        session_id: None,
    }
    .session_log_arguments();
    assert_eq!(typed_malformed["base_commit_valid"], false);
    assert!(typed_malformed["base_commit"].is_null());
    assert_eq!(typed_malformed["head_commit_valid"], true);
    assert!(!serde_json::to_string(&typed_malformed)
        .unwrap()
        .contains("source-like-invalid-value"));
    let definition =
        crate::tool_runtime::tool_definition::lookup_tool_definition("read_git_review_summary")
            .expect("read_git_review_summary definition");
    assert_eq!(
        definition.metadata.authority,
        crate::tool_runtime::metadata::ToolAuthorityPolicy::Require(SCOPE_PROJECT_READ)
    );
    assert!(definition.visibility.is_model_visible());
    let metadata =
        lookup_tool_metadata("read_git_review_summary").expect("read_git_review_summary metadata");
    assert_eq!(metadata.risk, ToolRisk::Read);
    assert_eq!(
        metadata.authority,
        crate::tool_runtime::metadata::ToolAuthorityPolicy::Require(SCOPE_PROJECT_READ)
    );
    assert!(metadata.requires_project);
    assert_eq!(
        metadata.effect,
        crate::tool_runtime::metadata::ToolEffect::Observe
    );
    assert!(!metadata.destructive);
    assert_eq!(
        oauth_scope_policy_for_runtime_tool("read_git_review_summary"),
        OAuthToolScopePolicy::Require(SCOPE_PROJECT_READ)
    );
    let specs = crate::tool_runtime::registered_tool_specs();
    let spec = specs
        .iter()
        .find(|spec| spec.name == "read_git_review_summary")
        .expect("read_git_review_summary public spec");
    assert!(spec
        .description
        .contains("Specialist exact committed-range review map"));
    assert!(spec
        .description
        .contains("Ordinary review uses review_changes"));
    for field in ["base_commit", "head_commit"] {
        assert_eq!(spec.input_schema["properties"][field]["minLength"], 40);
        assert_eq!(spec.input_schema["properties"][field]["maxLength"], 40);
        assert_eq!(
            spec.input_schema["properties"][field]["pattern"],
            "^[0-9A-Fa-f]{40}$"
        );
    }
    let output = crate::tool_runtime::registry::output_schema_for_tool("read_git_review_summary");
    let payload = &output["properties"]["output"];
    for field in [
        "scope",
        "stats",
        "file_classes",
        "subsystems",
        "signals",
        "files",
        "coverage",
        "bounds",
        "truncation",
        "deterministic",
        "llm_summary",
        "truncated",
        "warnings",
    ] {
        assert!(
            payload["properties"].get(field).is_some(),
            "missing output field {field}"
        );
    }
}

#[tokio::test]
async fn git_or_shell_tools_rejected_without_git_or_shell_capability() {
    let runtime = runtime_with_agent_project("oe");
    register_agent(
        &runtime,
        "oe",
        None,
        RunnerCapabilities {
            shell: false,
            ..Default::default()
        },
    )
    .await;
    let bootstrap = auth_context(None, true);

    let calls = [
        ToolCall::GitReviewSummary {
            project: agent_test_project_id("oe"),
            base_commit: "a".repeat(40),
            head_commit: "b".repeat(40),
            session_id: None,
        },
        ToolCall::ShowChanges {
            project: agent_test_project_id("oe"),
            session_id: None,
            include_diff: None,
            max_hunks: None,
            max_hunk_lines: None,
            session_event_limit: None,
        },
    ];
    for call in calls {
        let name = format!("{:?}", call);
        let result = runtime.dispatch_with_auth(call, Some(&bootstrap)).await;
        assert!(!result.success, "{name} should be rejected");
        let err = result.error.unwrap();
        assert!(
            err.contains("shell") || err.contains("git"),
            "{name} should require shell or git capability: {err}",
        );
    }
}

#[test]
fn show_changes_status_observation_distinguishes_failure_classes() {
    let observed = parse_show_changes_status_observation(
        "## main",
        "status_exit=0\nrepository_probe=inside_worktree\nrepository_probe_exit=0",
        "",
    );
    assert_eq!(observed.as_json()["status"], "observed");

    let non_git = parse_show_changes_status_observation(
        "",
        "status_exit=128\nrepository_probe=outside_worktree\nrepository_probe_exit=128",
        "fatal: not a git repository",
    );
    assert_eq!(non_git.as_json()["status"], "non_git");
    assert_eq!(non_git.as_json()["reason_code"], "not_a_git_repository");

    let config_failure = parse_show_changes_status_observation(
        "",
        "status_exit=128\nrepository_probe=inside_worktree\nrepository_probe_exit=0",
        "fatal: bad config variable 'status.showuntrackedfiles'",
    );
    assert_eq!(config_failure.as_json()["status"], "command_failed");
    assert_eq!(
        config_failure.as_json()["reason_code"],
        "git_status_config_error"
    );
    assert_eq!(
        config_failure.as_json()["repository_probe"],
        "inside_worktree"
    );

    let permission_failure = parse_show_changes_status_observation(
        "",
        "status_exit=128\nrepository_probe=unavailable\nrepository_probe_exit=128",
        "fatal: Permission denied",
    );
    assert_eq!(permission_failure.as_json()["status"], "command_failed");
    assert_eq!(
        permission_failure.as_json()["reason_code"],
        "git_status_permission_denied"
    );

    let unavailable = parse_show_changes_status_observation(
        "",
        "status_exit=0\nrepository_probe=inside_worktree\nrepository_probe_exit=0",
        "",
    );
    assert_eq!(unavailable.as_json()["status"], "output_unavailable");
    assert_eq!(
        unavailable.as_json()["reason_code"],
        "git_status_header_unavailable"
    );
}

async fn run_show_changes_via_agent(
    runtime: &ToolRuntime,
    client_id: &str,
    project: String,
    session_id: Option<String>,
    include_diff: bool,
) -> ToolResult {
    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .show_changes(project, session_id, Some(include_diff), None, None, None)
            .await
    });
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    while !task.is_finished() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "read_workspace_changes did not finish within 10 seconds for client {client_id}"
        );
        if let Some(req) = probe_patch_agent_request(runtime, client_id).await {
            assert_eq!(req.kind, "run_internal_posix_script");
            assert!(req.command.is_empty());
            let payload = req
                .script
                .as_ref()
                .expect("read_workspace_changes must carry a typed internal script");
            assert_eq!(
                payload.language,
                crate::runner_protocol::ShellScriptLanguage::Sh
            );
            assert!(payload.args.is_empty());
            complete_agent_request_by_running_locally(runtime, client_id, req).await;
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
    task.await.unwrap()
}

async fn run_show_changes_for_presentation_via_agent(
    runtime: &ToolRuntime,
    client_id: &str,
    project: String,
) -> ToolResult {
    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .workspace_metadata_for_presentation(project)
            .await
    });
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    while !task.is_finished() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "show_changes_for_presentation did not finish within 10 seconds for client {client_id}"
        );
        if let Some(req) = probe_patch_agent_request(runtime, client_id).await {
            assert_eq!(req.kind, "run_internal_posix_script");
            assert!(req.command.is_empty());
            let payload = req
                .script
                .as_ref()
                .expect("show_changes_for_presentation must carry a typed internal script");
            assert_eq!(
                payload.language,
                crate::runner_protocol::ShellScriptLanguage::Sh
            );
            assert!(payload.args.is_empty());
            complete_agent_request_by_running_locally(runtime, client_id, req).await;
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
    task.await.unwrap()
}

fn framed_block(kind: char, body: &str, metadata: &str) -> String {
    crate::tool_runtime::git::framed_show_changes_test_block(kind, body, metadata)
}

#[test]
fn show_changes_modern_framing_requires_exact_blocks_and_tail() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "before\n", "initial");
    std::fs::write(tmp.path().join("README.md"), "after\n").unwrap();

    for include_diff in [false, true] {
        let (_, stdout, stderr) = run_bounded_show_changes_full(tmp.path(), include_diff, 20, 80);
        assert_eq!(
            stdout.matches("WCSF1:").count(),
            if include_diff { 5 } else { 4 }
        );
        let frames = split_show_changes_stdout(&stdout, include_diff);
        assert!(frames.framing_valid);
        let output = bounded_show_changes_output_from_frames(
            &frames,
            tmp.path(),
            include_diff,
            20,
            80,
            &stderr,
        );
        assert_eq!(output["transport_safe"], true, "{output}");
        assert_eq!(output["files"][0]["path"], "README.md");
        assert_eq!(output["files"][0]["additions"], 1);
        assert_eq!(output["files"][0]["deletions"], 1);
    }

    let (_, valid, stderr) = run_bounded_show_changes_full(tmp.path(), false, 20, 80);
    let trailer = valid.rfind("WCSF1:N:").unwrap();
    let mut variants = Vec::new();
    variants.push(("extra_tail", format!("{valid}x")));
    variants.push(("missing_tail", valid[..valid.len() - 1].to_string()));
    let mut invalid_header = valid.clone().into_bytes();
    invalid_header[trailer] = b'X';
    variants.push(("invalid_header", String::from_utf8(invalid_header).unwrap()));
    let mut length_mismatch = valid.clone().into_bytes();
    length_mismatch[trailer + 8] = if length_mismatch[trailer + 8] == b'9' {
        b'8'
    } else {
        b'9'
    };
    variants.push((
        "length_mismatch",
        String::from_utf8(length_mismatch).unwrap(),
    ));
    let mut body_early_end = valid.into_bytes();
    body_early_end[trailer + 8..trailer + 18].copy_from_slice(b"9999999999");
    variants.push(("body_early_end", String::from_utf8(body_early_end).unwrap()));

    for (label, malformed) in variants {
        let frames = split_show_changes_stdout(&malformed, false);
        assert!(!frames.framing_valid, "{label}");
        let output =
            bounded_show_changes_output_from_frames(&frames, tmp.path(), false, 20, 80, &stderr);
        assert_eq!(output["transport_safe"], false, "{label}: {output}");
    }

    let synthetic = format!(
        "{}{}{}{}",
        framed_block('S', "## main\n", "status_exit=0\n"),
        framed_block('H', "", "head_exit=1\n"),
        framed_block('T', "", "diff_stat_exit=0\n"),
        framed_block('N', "", "numstat_exit=0\n")
    );
    assert!(split_show_changes_stdout(&synthetic, false).framing_valid);

    let legacy = "## main\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nabc123\0abc123\0head\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n";
    let legacy_frames = split_show_changes_stdout(legacy, false);
    assert!(!legacy_frames.framing_valid);
    assert!(legacy_frames.status.is_empty());
    assert!(legacy_frames.head.is_empty());
    let legacy_head = parse_show_changes_output(
        "demo",
        "## main",
        "abc123\0abc123\0head",
        "",
        None,
        20,
        80,
        Some(0),
        "",
    );
    assert!(legacy_head["head"]["commit"].is_null());
    assert!(legacy_head["head"]["short"].is_null());
    assert!(legacy_head["head"]["summary"].is_null());
}

#[test]
fn show_changes_numstat_does_not_report_rename_as_full_line_churn() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "old.txt", "one\ntwo\nthree\n", "initial");
    std::fs::rename(tmp.path().join("old.txt"), tmp.path().join("new.txt")).unwrap();
    let (add_exit, _, add_stderr, _) = run_command_sync("git add -A", tmp.path(), 30);
    assert_eq!(add_exit, 0, "git add failed: {add_stderr}");

    let (_, stdout, stderr) = run_bounded_show_changes_full(tmp.path(), false, 20, 80);
    let frames = split_show_changes_stdout(&stdout, false);
    assert!(frames.framing_valid);
    let output =
        bounded_show_changes_output_from_frames(&frames, tmp.path(), false, 20, 80, &stderr);
    assert_eq!(output["transport_safe"], true, "{output}");
    assert_eq!(output["counts"]["renamed"], 1, "{output}");
    let file = output["files"]
        .as_array()
        .and_then(|files| files.iter().find(|file| file["status"] == "renamed"))
        .expect("rename record");
    assert!(file.get("additions").is_none(), "{file}");
    assert!(file.get("deletions").is_none(), "{file}");
}

#[tokio::test]
async fn show_changes_preserves_sentinel_text_in_normal_diff_and_tool_result() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "collision.txt", "before\n", "initial");
    std::fs::write(
        tmp.path().join("collision.txt"),
        format!("before\n{SHOW_CHANGES_SENTINEL}\nprefix{SHOW_CHANGES_SENTINEL}suffix\n"),
    )
    .unwrap();

    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "show-collision", "demo", tmp.path()).await;
    let result = run_show_changes_via_agent(&runtime, "show-collision", project, None, true).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["diff_exit"], 0);
    assert_eq!(result.output["transport_safe"], true);
    let serialized = result.output.to_string();
    assert!(serialized.contains(&format!("+{SHOW_CHANGES_SENTINEL}")));
    assert!(serialized.contains(&format!("+prefix{SHOW_CHANGES_SENTINEL}suffix")));
    assert_show_changes_envelope_matches_schema("sentinel normal diff", &result);
}

#[tokio::test]
async fn show_changes_agent_untracked_preview_uses_internal_posix_runtime() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "tracked\n", "initial");
    std::fs::write(tmp.path().join("notes.txt"), "alpha\nbeta\n").unwrap();

    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "show-untracked", "demo", tmp.path()).await;
    let result = run_show_changes_via_agent(&runtime, "show-untracked", project, None, true).await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["counts"]["untracked"], 1);
    let preview = preview_for_path(&result.output, "notes.txt");
    assert_eq!(preview["kind"], "text");
    assert_eq!(preview["lines"][0]["text"], "alpha");
    assert_eq!(preview["lines"][1]["text"], "beta");
}

#[cfg(unix)]
#[test]
fn show_changes_preserves_sentinel_text_from_external_diff() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = crate::test_support::executable_tempdir();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "external.txt", "before\n", "initial");
    std::fs::write(tmp.path().join("external.txt"), "after\n").unwrap();
    let script = tmp.path().join("external-diff.sh");
    std::fs::write(
        &script,
        format!("#!/bin/sh\nprintf '%s\\n' '{SHOW_CHANGES_SENTINEL}'\nexit 0\n"),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();
    let config = format!(
        "git config diff.external {}",
        shell_single_quote(script.to_str().unwrap())
    );
    let (exit, _, stderr) = run_command_full_capture(&config, tmp.path(), 30);
    assert_eq!(exit, 0, "{stderr}");

    let (_, stdout, stderr) = run_bounded_show_changes_full(tmp.path(), true, 20, 80);
    let frames = split_show_changes_stdout(&stdout, true);
    assert!(frames.framing_valid);
    assert_eq!(frames.diff_exit, Some(0));
    assert_eq!(frames.diff, SHOW_CHANGES_SENTINEL);
    let output =
        bounded_show_changes_output_from_frames(&frames, tmp.path(), true, 20, 80, &stderr);
    assert_eq!(output["transport_safe"], true, "{output}");
}

#[test]
fn show_changes_preserves_sentinel_text_in_head_subject() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let subject = format!("subject {SHOW_CHANGES_SENTINEL} suffix");
    commit_file(tmp.path(), "README.md", "hello\n", &subject);
    let (commit_exit, expected_commit, commit_stderr) =
        run_command_full_capture("git rev-parse HEAD", tmp.path(), 30);
    assert_eq!(commit_exit, 0, "{commit_stderr}");
    let (short_exit, expected_short, short_stderr) =
        run_command_full_capture("git rev-parse --short HEAD", tmp.path(), 30);
    assert_eq!(short_exit, 0, "{short_stderr}");
    let expected_commit = expected_commit.trim();
    let expected_short = expected_short.trim();

    let (_, stdout, stderr) = run_bounded_show_changes_full(tmp.path(), false, 20, 80);
    let frames = split_show_changes_stdout(&stdout, false);
    let output =
        bounded_show_changes_output_from_frames(&frames, tmp.path(), false, 20, 80, &stderr);
    assert_eq!(output["head"]["commit"], expected_commit);
    assert_eq!(output["head"]["short"], expected_short);
    assert_eq!(output["head"]["summary"], subject);
    assert_eq!(output["transport_safe"], true, "{output}");
}

#[test]
fn show_changes_preserves_sentinel_text_in_tracked_filename() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let filename = format!("tracked-{SHOW_CHANGES_SENTINEL}-file.txt");
    commit_file(tmp.path(), &filename, "before\n", "initial");
    std::fs::write(tmp.path().join(&filename), "after\n").unwrap();

    let (_, stdout, stderr) = run_bounded_show_changes_full(tmp.path(), true, 20, 80);
    let frames = split_show_changes_stdout(&stdout, true);
    assert!(frames.status.contains(&filename));
    assert!(frames.stat.contains(&filename));
    assert!(frames.diff.contains(&filename));
    let output =
        bounded_show_changes_output_from_frames(&frames, tmp.path(), true, 20, 80, &stderr);
    assert_eq!(output["transport_safe"], true, "{output}");
    assert!(output["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|file| file["path"] == filename));
    assert_show_changes_envelope_value_matches_schema(&output, "sentinel filename");
}

fn assert_show_changes_envelope_matches_schema(label: &str, result: &ToolResult) {
    let schema = crate::tool_runtime::registry::output_schema_for_tool("read_workspace_changes");
    let envelope = json!({
        "success": result.success,
        "output": result.output,
        "error": result.error,
    });
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&envelope, &schema)
        .unwrap_or_else(|error| {
            panic!("{label} read_workspace_changes schema mismatch: {error}\n{envelope}")
        });
}

/// Validate a bare `read_workspace_changes` output `Value` against the output schema by
/// wrapping it in the success envelope.
fn assert_show_changes_envelope_value_matches_schema(output: &Value, label: &str) {
    let schema = crate::tool_runtime::registry::output_schema_for_tool("read_workspace_changes");
    let envelope = json!({
        "success": true,
        "output": output,
        "error": Value::Null,
    });
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&envelope, &schema)
        .unwrap_or_else(|error| {
            panic!("{label} read_workspace_changes schema mismatch: {error}\n{envelope}")
        });
}

/// Run the production-bounded `read_workspace_changes` command in a local repo and
/// parse it into the structured `read_workspace_changes` output value, the same way the
/// runtime does (without the separate untracked-preview collection).
fn bounded_show_changes_output(
    root: &Path,
    include_diff: bool,
    max_hunks: usize,
    max_hunk_lines: usize,
) -> Value {
    let cmd = show_changes_command(include_diff, max_hunks, max_hunk_lines);
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, root, 30);
    let frames = split_show_changes_stdout(&stdout, include_diff);
    let observation =
        parse_show_changes_status_observation(&frames.status, &frames.status_result, &stderr);
    let effective_exit = if observation.exit_code != Some(0) {
        observation.exit_code
    } else {
        Some(exit_code)
    };
    parse_show_changes_output_with_observation(
        "demo",
        &frames.status,
        &frames.head,
        &frames.stat,
        include_diff.then_some(frames.diff.as_str()),
        max_hunks,
        max_hunk_lines,
        effective_exit,
        &stderr,
        observation,
        &frames,
    )
}

/// Mirror the ordinary Runner per-stream result-retention tail behavior used by
/// the current 256 KiB compatibility floor. This is intentionally not a model
/// result ceiling or a polling/WebSocket/QUIC wire/body/frame bound.
fn simulate_runner_result_retention_tail(stdout: &str, max_bytes: usize) -> String {
    if stdout.len() <= max_bytes {
        return stdout.to_string();
    }
    let mut start = stdout.len() - max_bytes;
    while start < stdout.len() && !stdout.is_char_boundary(start) {
        start += 1;
    }
    format!(
        "[output truncated to last {} bytes]\n{}",
        max_bytes,
        &stdout[start..]
    )
}

/// Run a shell command and fully capture its stdout/stderr without the pipe
/// deadlock that affects `run_command_sync` for outputs above the OS pipe
/// buffer (~64 KiB). The stdout/stderr pipes are drained on dedicated threads
/// so a large-output git command can write freely while the main thread polls
/// the child status. Used by large-output regression tests where the command
/// legitimately exceeds the pipe buffer.
fn run_command_full_capture(cmd: &str, cwd: &Path, timeout_secs: u64) -> (i32, String, String) {
    use std::io::Read;
    #[cfg(windows)]
    use std::io::Write;
    use std::process::Command;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    let mut command = Command::new(crate::tool_runtime::helpers::test_shell());
    #[cfg(windows)]
    command.arg("-s").stdin(std::process::Stdio::piped());
    #[cfg(not(windows))]
    command.arg("-c").arg(cmd);
    command
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(error) => return (-1, String::new(), format!("failed to spawn: {error}")),
    };
    #[cfg(windows)]
    {
        let write_result = child
            .stdin
            .take()
            .expect("test shell stdin")
            .write_all(cmd.as_bytes());
        if let Err(error) = write_result {
            let _ = child.kill();
            let _ = child.wait();
            return (
                -1,
                String::new(),
                format!("failed to write shell command: {error}"),
            );
        }
    }
    let mut stdout = child.stdout.take().expect("stdout piped");
    let mut stderr = child.stderr.take().expect("stderr piped");
    // Drain each pipe on its own thread so the child never blocks on a full
    // pipe while we wait for it to exit.
    let (tx_out, rx_out) = mpsc::channel::<std::io::Result<Vec<u8>>>();
    let (tx_err, rx_err) = mpsc::channel::<std::io::Result<Vec<u8>>>();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let res = stdout.read_to_end(&mut buf).map(|_| buf);
        let _ = tx_out.send(res);
    });
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let res = stderr.read_to_end(&mut buf).map(|_| buf);
        let _ = tx_err.send(res);
    });
    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return (
                    -1,
                    String::new(),
                    format!("Command timed out after {timeout_secs} seconds"),
                );
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return (-1, String::new(), "failed to wait".to_string()),
        }
    };
    let stdout = rx_out
        .recv()
        .map(|r| r.unwrap_or_default())
        .unwrap_or_default();
    let stderr = rx_err
        .recv()
        .map(|r| r.unwrap_or_default())
        .unwrap_or_default();
    (
        status.code().unwrap_or(-1),
        String::from_utf8_lossy(&stdout).into_owned(),
        String::from_utf8_lossy(&stderr).into_owned(),
    )
}

#[tokio::test]
async fn show_changes_degrades_gracefully_for_non_git_project() {
    let tmp = tempfile::tempdir().unwrap();
    // Intentionally do NOT init a git repo.
    let runtime = test_runtime();
    let project = register_runner_project_at_path(&runtime, "ng", "demo", tmp.path()).await;
    let result = run_show_changes_via_agent(&runtime, "ng", project, None, false).await;
    assert!(
        result.success,
        "non-git project must not be a runtime failure: {:?}",
        result.error
    );
    assert_eq!(result.output["non_git_project"], true);
    assert_eq!(result.output["git_available"], false);
    let git_error = result.output["git_error"].as_str().unwrap_or_default();
    assert!(
        git_error.contains("not a git repository"),
        "unexpected git_error: {git_error}"
    );
    // No full git usage/fatal stderr must leak into the user-facing payload.
    assert_eq!(result.output["stderr"], "");
    let serialized = serde_json::to_string(&result.output).unwrap();
    assert!(
        !serialized.contains("--no-index"),
        "leaked git diff usage: {serialized}"
    );
    assert!(
        !serialized.contains("usage") && !serialized.contains("用法"),
        "leaked git usage: {serialized}"
    );
    assert!(result.output["files"].as_array().unwrap().is_empty());
    assert!(result.output["session"].is_null());
    assert_review_verdict_shape(&result.output["verdict"]);
    assert_eq!(result.output["verdict"]["status"], "warn");
    assert_eq!(result.output["verdict"]["blocking"], false);
    assert_reason_list_contains(
        &result.output["verdict"],
        "warning_reasons",
        "non_git_project",
    );
    let actions = result.output["suggested_next_actions"].as_array().unwrap();
    assert!(actions
        .iter()
        .any(|a| a.as_str().unwrap().contains("not applicable")));
    assert_eq!(result.output["status_observation"]["status"], "non_git");
    assert_eq!(
        result.output["head"],
        json!({
            "commit": null,
            "short": null,
            "summary": null,
        })
    );
    assert_show_changes_envelope_matches_schema("non-git", &result);
}

#[tokio::test]
async fn show_changes_non_git_project_still_returns_session_summary() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project = register_runner_project_at_path(&runtime, "ngs", "demo", tmp.path()).await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("task".to_string()));
    let args = json!({"project": project, "path": "src/foo.rs"});
    let start = runtime.sessions.record_tool_call_started(
        Some(&session.session_id),
        crate::tool_runtime::sessions::SessionTransport::Api,
        "write_project_file",
        &args,
        crate::tool_runtime::sessions::session_tool_contract("write_project_file"),
    );
    runtime
        .sessions
        .record_tool_call_finished(start, true, &json!({}), None, None);

    let result = run_show_changes_via_agent(
        &runtime,
        "ngs",
        project,
        Some(session.session_id.clone()),
        false,
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["non_git_project"], true);
    assert_eq!(result.output["git_available"], false);
    assert_eq!(result.output["session"]["found"], true);
    assert_eq!(result.output["session"]["session_id"], session.session_id);
    assert!(result.output["session"].get("recent_events").is_none());
    assert_eq!(result.output["session"]["signals"]["write_like"], true);
    assert_eq!(
        result.output["session"]["changed_paths"],
        json!(["src/foo.rs"])
    );
    // Session-signal suggestions are layered on top of the git-unavailable hint.
    let actions = result.output["suggested_next_actions"].as_array().unwrap();
    assert!(actions
        .iter()
        .any(|a| a.as_str().unwrap().contains("unavailable")));
    assert!(actions
        .iter()
        .any(|a| a.as_str().unwrap().contains("review changed paths")));
}

#[tokio::test]
async fn show_changes_real_git_repo_marks_git_available_and_reports_status() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    let runtime = test_runtime();
    let project = register_runner_project_at_path(&runtime, "gr", "demo", tmp.path()).await;
    let result = run_show_changes_via_agent(&runtime, "gr", project, None, false).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["non_git_project"], false);
    assert_eq!(result.output["git_available"], true);
    assert_eq!(result.output["git_error"], serde_json::Value::Null);
    assert_eq!(result.output["clean"], true);
    assert_review_verdict_shape(&result.output["verdict"]);
    assert_ne!(result.output["verdict"]["status"], "fail");
    assert_eq!(result.output["verdict"]["blocking"], false);
    assert!(result.output["branch"].as_str().is_some());
    assert!(result.output["head"]["short"].as_str().is_some());
    assert_eq!(result.output["counts"]["modified"], 0);
    assert!(result.output["files"].as_array().unwrap().is_empty());
    // No git-unavailable suggestion for a real repo.
    let actions = result.output["suggested_next_actions"].as_array().unwrap();
    assert!(!actions
        .iter()
        .any(|a| a.as_str().unwrap().contains("unavailable")));
    assert_eq!(result.output["status_observation"]["status"], "observed");
    assert_show_changes_envelope_matches_schema("git include_diff=false", &result);
}

#[tokio::test]
async fn show_changes_real_git_repo_include_diff_true_matches_schema() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    std::fs::write(tmp.path().join("README.md"), "hello\nchanged\n").unwrap();
    let runtime = test_runtime();
    let project = register_runner_project_at_path(&runtime, "grd", "demo", tmp.path()).await;
    let result = run_show_changes_via_agent(&runtime, "grd", project, None, true).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["status_observation"]["status"], "observed");
    assert_eq!(result.output["clean"], false);
    assert!(result.output["hunk_count"].as_u64().unwrap_or(0) > 0);
    assert_show_changes_envelope_matches_schema("git include_diff=true", &result);
}

#[tokio::test]
async fn show_changes_presentation_preserves_staging_without_eager_diff() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    std::fs::write(tmp.path().join("README.md"), "hello\nstaged\n").unwrap();
    let (exit_code, _, stderr, _) = run_command_sync("git add README.md", tmp.path(), 30);
    assert_eq!(exit_code, 0, "git add failed: {stderr}");

    let runtime = test_runtime();
    let project = register_runner_project_at_path(&runtime, "grp", "demo", tmp.path()).await;
    let result = run_show_changes_for_presentation_via_agent(&runtime, "grp", project).await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["clean"], false);
    assert_eq!(result.output["files"][0]["path"], "README.md");
    assert_eq!(result.output["files"][0]["staged"], true);
    assert_eq!(result.output["hunk_count"].as_u64().unwrap_or(0), 0);
    assert!(result.output["hunks"].as_array().is_none_or(Vec::is_empty));
    assert!(!result.output.to_string().contains("+staged"));
}

#[tokio::test]
async fn show_changes_status_failure_is_not_masked_by_successful_diff() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    std::fs::write(tmp.path().join("README.md"), "hello\nchanged\n").unwrap();
    let (config_exit, _, config_stderr, _) = run_command_sync(
        "git config status.showUntrackedFiles invalid",
        tmp.path(),
        30,
    );
    assert_eq!(
        config_exit, 0,
        "failed to set regression config: {config_stderr}"
    );

    let runtime = test_runtime();
    let project = register_runner_project_at_path(&runtime, "gsf", "demo", tmp.path()).await;
    let result = run_show_changes_via_agent(&runtime, "gsf", project, None, false).await;
    assert!(
        !result.success,
        "status failure must fail read_workspace_changes"
    );
    assert_eq!(
        result.output["status_observation"]["status"],
        "command_failed"
    );
    assert_eq!(
        result.output["status_observation"]["reason_code"],
        "git_status_config_error"
    );
    assert_eq!(
        result.output["status_observation"]["repository_probe"],
        "inside_worktree"
    );
    assert_eq!(result.output["git_available"], true);
    assert_eq!(result.output["non_git_project"], false);
    assert_eq!(result.output["clean"], Value::Null);
    assert_eq!(result.output["counts"]["conflicted"], Value::Null);
    assert!(
        result.output["diff_stat"]
            .as_str()
            .unwrap_or_default()
            .contains("README.md"),
        "successful diff output should remain available: {}",
        result.output
    );
    assert!(result.output["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["reason_code"] == "git_status_config_error"));
    assert_show_changes_envelope_matches_schema("status command failure", &result);
}

#[test]
fn show_changes_parses_upstream_observation_states() {
    let parse = |status: &str| {
        parse_show_changes_output(
            "agent:oe:webcodex",
            status,
            "commit=b47e4fb000000000000000000000000000000000\nshort=b47e4fb\nsummary=head",
            "",
            None,
            20,
            80,
            Some(0),
            "",
        )
    };

    let synced = parse("## main...origin/main");
    assert_eq!(synced["upstream_status"], "available");
    assert_eq!(synced["upstream_reason_code"], Value::Null);
    assert_eq!(synced["upstream"], "origin/main");
    assert_eq!(synced["ahead"], 0);
    assert_eq!(synced["behind"], 0);

    let diverged = parse("## main...origin/main [ahead 3, behind 2]");
    assert_eq!(diverged["upstream_status"], "available");
    assert_eq!(diverged["ahead"], 3);
    assert_eq!(diverged["behind"], 2);

    let gone = parse("## main...origin/main [gone]");
    assert_eq!(gone["upstream_status"], "gone");
    assert_eq!(gone["upstream_reason_code"], "upstream_gone");
    assert_eq!(gone["upstream"], "origin/main");
    assert_eq!(gone["ahead"], Value::Null);
    assert_eq!(gone["behind"], Value::Null);

    let absent = parse("## main");
    assert_eq!(absent["upstream_status"], "absent");
    assert_eq!(absent["upstream_reason_code"], Value::Null);
    assert_eq!(absent["upstream"], Value::Null);
    assert_eq!(absent["ahead"], Value::Null);
    assert_eq!(absent["behind"], Value::Null);
}

#[test]
fn show_changes_parses_unborn_and_detached_branch_headers() {
    for status in ["## No commits yet on main", "## Initial commit on main"] {
        let output = parse_show_changes_output(
            "agent:oe:webcodex",
            status,
            "",
            "",
            None,
            20,
            80,
            Some(0),
            "",
        );
        assert_eq!(output["branch"], "main", "status: {status}");
        assert_eq!(output["head"]["commit"], Value::Null);
        assert_eq!(output["upstream_status"], "absent");
    }

    for status in ["## HEAD (no branch)", "## HEAD (detached at b47e4fb)"] {
        let output = parse_show_changes_output(
            "agent:oe:webcodex",
            status,
            "commit=b47e4fb000000000000000000000000000000000\nshort=b47e4fb\nsummary=head",
            "",
            None,
            20,
            80,
            Some(0),
            "",
        );
        assert_eq!(output["branch"], Value::Null, "status: {status}");
        assert_eq!(output["upstream_status"], "absent");
    }
}

#[test]
fn non_git_show_changes_preserves_unobserved_state() {
    let output = non_git_show_changes_payload("agent:oe:plain", Some(128), false);
    assert_eq!(output["git_available"], false);
    assert_eq!(output["clean"], Value::Null);
    assert_eq!(output["counts"]["conflicted"], Value::Null);
    assert_eq!(output["upstream_status"], "unobserved");
    assert_eq!(output["upstream_reason_code"], "non_git_project");
    assert_eq!(output["upstream"], Value::Null);
    assert_eq!(output["ahead"], Value::Null);
    assert_eq!(output["behind"], Value::Null);
    assert_reason_list_contains(&output["verdict"], "warning_reasons", "non_git_project");
}

#[test]
fn show_changes_schema_explicitly_models_upstream_and_nullable_observation() {
    let schema = crate::tool_runtime::registry::output_schema_for_tool("read_workspace_changes");
    let properties = schema["properties"]["output"]["properties"]
        .as_object()
        .expect("read_workspace_changes output properties");
    for field in [
        "upstream_status",
        "upstream_reason_code",
        "upstream",
        "ahead",
        "behind",
    ] {
        assert!(
            properties.contains_key(field),
            "missing explicit field {field}"
        );
    }
    assert_eq!(
        properties["upstream_status"]["enum"],
        json!(["available", "absent", "gone", "unobserved"])
    );
    assert_eq!(properties["head"]["additionalProperties"], false);
    assert_eq!(properties["counts"]["additionalProperties"], false);
    assert!(properties["clean"]["anyOf"].is_array());
    assert!(properties["counts"]["properties"]["conflicted"]["anyOf"].is_array());
}

fn assert_review_verdict_shape(verdict: &serde_json::Value) {
    let status = verdict["status"].as_str().expect("status string");
    assert!(
        matches!(status, "pass" | "warn" | "fail"),
        "unexpected verdict status {status}: {verdict}"
    );
    assert!(verdict["blocking"].is_boolean(), "blocking bool: {verdict}");
    for key in [
        "blocking_reasons",
        "warning_reasons",
        "suggested_next_actions",
    ] {
        assert!(verdict[key].is_array(), "{key} array: {verdict}");
    }
}

fn assert_reason_list_contains(verdict: &serde_json::Value, key: &str, reason: &str) {
    let reasons = verdict[key].as_array().expect("reason list");
    assert!(
        reasons.iter().any(|value| value.as_str() == Some(reason)),
        "{key} should contain {reason}: {verdict}"
    );
}

fn assert_verdict_omits_raw_output_and_sensitive_values(
    verdict: &serde_json::Value,
    forbidden_values: &[&str],
    context: &str,
) {
    let serialized = serde_json::to_string(verdict).unwrap();
    for forbidden in ["stdout", "stderr", "tail", "excerpt", "command"] {
        assert!(
            !serialized.contains(forbidden),
            "{context} leaked raw output marker {forbidden}: {serialized}"
        );
    }
    for forbidden in forbidden_values {
        assert!(
            !serialized.contains(forbidden),
            "{context} leaked sensitive value {forbidden}: {serialized}"
        );
    }
}

/// Helper: run the bounded read_workspace_changes command with full stdout/stderr capture
/// (large outputs exceed the pipe buffer) and return (stdout_bytes, stdout).
fn run_bounded_show_changes_full(
    root: &Path,
    include_diff: bool,
    max_hunks: usize,
    max_hunk_lines: usize,
) -> (usize, String, String) {
    let cmd = show_changes_command(include_diff, max_hunks, max_hunk_lines);
    let (exit, stdout, stderr) = run_command_full_capture(&cmd, root, 30);
    assert_eq!(
        exit, 0,
        "read_workspace_changes failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    (stdout.len(), stdout, stderr)
}

fn near_path_max_diff_path(i: usize) -> std::path::PathBuf {
    let mut path = std::path::PathBuf::new();
    for level in 0..3 {
        path.push(format!("{}-{level:02}", "p".repeat(200)));
    }
    path.push(format!("{}-{i:02}.txt", "f".repeat(170)));
    path
}

#[test]
fn show_changes_long_path_diff_budgets_complete_preambles_and_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let total = 70usize;
    let paths: Vec<_> = (0..total).map(near_path_max_diff_path).collect();
    for path in &paths {
        let full = tmp.path().join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, "before\n").unwrap();
    }
    let (setup_exit, _, setup_stderr) =
        run_command_full_capture("git add -A && git commit -qm long-paths", tmp.path(), 60);
    assert_eq!(setup_exit, 0, "setup failed: {setup_stderr}");
    for (i, path) in paths.iter().enumerate() {
        std::fs::write(tmp.path().join(path), format!("before\nchanged-{i}\n")).unwrap();
    }
    let (raw_exit, raw_diff, raw_stderr) =
        run_command_full_capture("git diff --unified=80", tmp.path(), 60);
    assert_eq!(raw_exit, 0, "raw diff failed: {raw_stderr}");
    assert!(
        raw_diff.len() > 192 * 1024,
        "raw diff bytes={}",
        raw_diff.len()
    );

    let (stdout_bytes, stdout, stderr) = run_bounded_show_changes_full(tmp.path(), true, 80, 80);
    assert!(
        stdout_bytes <= SHOW_CHANGES_OUTPUT_BUDGET_BYTES,
        "bounded bytes={stdout_bytes}"
    );
    let frames = split_show_changes_stdout(&stdout, true);
    assert_eq!(frames.diff_exit, Some(0));
    assert_eq!(frames.diff_trunc_bytes, Some(true));
    assert_eq!(frames.diff_trunc_hunk_count, Some(false));
    assert_eq!(frames.diff_trunc_hunk_lines, Some(false));
    assert_eq!(frames.diff_trunc_bytes_in_hunk, Some(false));
    assert_eq!(frames.diff_bytes, Some(frames.diff.len()));

    let output =
        bounded_show_changes_output_from_frames(&frames, tmp.path(), true, 80, 80, &stderr);
    let reasons = output["truncation_reasons"].as_array().unwrap();
    assert!(
        reasons.iter().any(|r| r == "diff_byte_budget"),
        "reasons={reasons:?}"
    );
    assert!(!reasons.iter().any(|r| r == "diff_hunk_count_limit"));
    assert!(!reasons.iter().any(|r| r == "diff_hunk_line_limit"));

    assert_eq!(output["hunks_truncated"], true);
    assert_git_diff_hunks_recovery_call_parses(&output["diff_review_handoff"]["next_call"]);
    let actions = output["suggested_next_actions"].as_array().unwrap();
    assert!(actions.iter().any(|action| action
        == "follow read_git_diff_hunks recovery.later_hunks.next_call while has_more=true"));
    assert_show_changes_envelope_value_matches_schema(&output, "diff byte handoff");

    let mut rejected_seen = false;
    for (i, path) in paths.iter().enumerate() {
        let display = path.to_string_lossy().replace('\\', "/");
        let preamble = format!("diff --git a/{display} b/{display}");
        let body = format!("+changed-{i}");
        let accepted = frames.diff.contains(&preamble);
        assert_eq!(
            frames.diff.contains(&body),
            accepted,
            "partial/leaked record for {display}"
        );
        if !accepted {
            rejected_seen = true;
        }
    }
    assert!(
        rejected_seen,
        "fixture must reject at least one file by byte budget"
    );
    eprintln!(
        "long_path_diff raw_bytes={} bounded_bytes={} diff_frame_bytes={}",
        raw_diff.len(),
        stdout_bytes,
        frames.diff.len()
    );
}

#[test]
fn show_changes_hunk_limit_does_not_leak_subsequent_file_bodies_or_preambles() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    // 20 modified tracked files, each with one hunk. Cap at max_hunks=1 so only
    // the first file's first hunk should ever appear in the production output.
    for i in 0..20 {
        let name = format!("file{i}.txt");
        commit_file(tmp.path(), &name, "line\n", "init");
        std::fs::write(tmp.path().join(&name), "line\nmore\n").unwrap();
    }
    let (stdout_bytes, stdout, _stderr) = run_bounded_show_changes_full(tmp.path(), true, 1, 80);
    // The original production-side output must stay within the budget.
    assert!(
        stdout_bytes <= SHOW_CHANGES_OUTPUT_BUDGET_BYTES,
        "bounded stdout must stay within budget; got {stdout_bytes}"
    );
    // Exactly one selected hunk header may appear in the raw production output.
    let hunk_headers: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("@@ "))
        .collect();
    assert_eq!(
        hunk_headers.len(),
        1,
        "exactly one hunk must be selected; got {} headers: {stdout}",
        hunk_headers.len()
    );
    // Exactly one `diff --git` file preamble may appear (only the selected
    // file's preamble is flushed; unselected files keep no preamble).
    let file_headers: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("diff --git "))
        .collect();
    assert_eq!(
        file_headers.len(),
        1,
        "exactly one file preamble must appear; got {} headers: {stdout}",
        file_headers.len()
    );
    // The bodies of the other 19 files must not leak. Each file's content is
    // `line\nmore\n`; the diff would show `+more`. Only one `+more` may appear.
    let plus_more_count = stdout.lines().filter(|line| *line == "+more").count();
    assert_eq!(
        plus_more_count, 1,
        "only the selected file's body may appear; got {plus_more_count} '+more' lines: {stdout}"
    );
    // The structured output must report a single returned hunk with truncation.
    let frames = split_show_changes_stdout(&stdout, true);
    assert_eq!(frames.diff_hunks_returned, Some(1));
    assert_eq!(frames.diff_hunks_truncated, Some(true));
    assert_eq!(frames.diff_exit, Some(0));
    assert_eq!(frames.diff_trunc_hunk_count, Some(true));
    assert_eq!(frames.diff_trunc_hunk_lines, Some(false));
    assert_eq!(frames.diff_trunc_bytes, Some(false));
    assert_show_changes_envelope_value_matches_schema(
        &bounded_show_changes_output_from_frames(&frames, tmp.path(), true, 1, 80, &_stderr),
        "hunk limit leak",
    );
}

/// Parse already-captured frames into the structured read_workspace_changes output,
/// reusing the captured stderr (avoids re-running the command).
fn bounded_show_changes_output_from_frames(
    frames: &ShowChangesStdout,
    root: &Path,
    include_diff: bool,
    max_hunks: usize,
    max_hunk_lines: usize,
    stderr: &str,
) -> Value {
    let observation =
        parse_show_changes_status_observation(&frames.status, &frames.status_result, stderr);
    let effective_exit = if observation.exit_code != Some(0) {
        observation.exit_code
    } else {
        Some(0)
    };
    let mut output = parse_show_changes_output_with_observation(
        "demo",
        &frames.status,
        &frames.head,
        &frames.stat,
        include_diff.then_some(frames.diff.as_str()),
        max_hunks,
        max_hunk_lines,
        effective_exit,
        stderr,
        observation,
        frames,
    );
    if include_diff {
        let untracked_paths = show_changes_untracked_paths(&output);
        let (previews, truncated) =
            collect_show_changes_untracked_previews_for_root(root, &untracked_paths);
        output["untracked_previews"] = json!(previews);
        output["untracked_previews_truncated"] = json!(truncated);
    }
    output
}

#[cfg(unix)]
#[test]
fn show_changes_oversized_no_hunk_preamble_is_bounded_and_drained() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = crate::test_support::executable_tempdir();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "before\n", "initial");
    std::fs::write(tmp.path().join("README.md"), "after\n").unwrap();

    let payload_path = tmp.path().join("external-diff-payload.txt");
    let line = format!("{}\n", "P".repeat(256));
    let payload = line.repeat((SHOW_CHANGES_DIFF_BYTES / line.len()) + 40);
    std::fs::write(&payload_path, &payload).unwrap();
    let script_path = tmp.path().join("external-diff.sh");
    std::fs::write(
        &script_path,
        "#!/bin/sh\ncat \"$WEBCODEX_TEST_DIFF_PAYLOAD\"\nexit 7\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&script_path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script_path, permissions).unwrap();

    let env = format!(
        "export WEBCODEX_TEST_DIFF_PAYLOAD={}; export GIT_EXTERNAL_DIFF={};",
        shell_single_quote(payload_path.to_str().unwrap()),
        shell_single_quote(script_path.to_str().unwrap())
    );
    let (raw_exit, raw_diff, raw_stderr) =
        run_command_full_capture(&format!("{env} git diff --unified=80"), tmp.path(), 30);
    assert_ne!(raw_exit, 0, "external diff must fail: {raw_stderr}");
    assert!(
        raw_diff.len() > SHOW_CHANGES_DIFF_BYTES,
        "raw no-hunk bytes={} budget={SHOW_CHANGES_DIFF_BYTES}",
        raw_diff.len()
    );
    assert!(!raw_diff.lines().any(|line| line.starts_with("@@ ")));

    let command = show_changes_command(true, 20, 80);
    let (exit, stdout, stderr) =
        run_command_full_capture(&format!("{env} {command}"), tmp.path(), 30);
    assert_eq!(exit, 0, "bounded command failed: {stderr}\n{stdout}");
    assert!(
        stdout.len() <= SHOW_CHANGES_OUTPUT_BUDGET_BYTES,
        "bounded stdout bytes={}",
        stdout.len()
    );
    let frames = split_show_changes_stdout(&stdout, true);
    assert!(
        frames.diff.is_empty(),
        "partial preamble leaked: {}",
        frames.diff
    );
    assert_eq!(frames.diff_bytes, Some(0));
    assert_eq!(frames.diff_trunc_bytes, Some(true));
    assert_eq!(frames.diff_trunc_hunk_count, Some(false));
    assert_eq!(frames.diff_trunc_hunk_lines, Some(false));
    assert_eq!(frames.diff_exit, Some(raw_exit));

    let output =
        bounded_show_changes_output_from_frames(&frames, tmp.path(), true, 20, 80, &stderr);
    assert_eq!(output["transport_safe"], true, "{output}");
    assert_eq!(
        output["truncation_reasons"],
        json!(["diff_byte_budget"]),
        "only the byte budget fired: {output}"
    );
    eprintln!(
        "oversized_no_hunk raw_bytes={} bounded_stdout_bytes={} diff_exit={} reasons={}",
        raw_diff.len(),
        stdout.len(),
        raw_exit,
        output["truncation_reasons"]
    );
}

#[test]
fn show_changes_propagates_real_full_diff_exit_status() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    std::fs::write(tmp.path().join("README.md"), "hello\nchanged\n").unwrap();
    // An external diff program that exits non-zero only for the full diff
    // (it is invoked by `git diff`, not by `git diff --stat`). It writes a
    // marker so we can confirm it was actually run.
    let marker = tmp.path().join("extdiff_ran.txt");
    let marker_str = marker.to_str().unwrap();
    let ext_diff = format!(
        "sh -c 'printf x > {marker_str}; exit 7' \"$1\" \"$2\" \"$3\" \"$4\" \"$5\" \"$6\" \"$7\""
    );
    // `git diff --stat` does not invoke the external diff (it computes stats
    // from the diffcore), so it should still return 0.
    let (stat_exit, _stat_out, stat_err) = run_command_full_capture(
        &format!("GIT_EXTERNAL_DIFF={ext_diff:?} git diff --stat"),
        tmp.path(),
        30,
    );
    assert_eq!(
        stat_exit, 0,
        "git diff --stat should return 0 even with a failing external diff: {stat_err}"
    );
    // The full `git diff` must return non-zero because the external diff fails.
    let (diff_exit, _diff_out, diff_err) = run_command_full_capture(
        &format!("GIT_EXTERNAL_DIFF={ext_diff:?} git diff --unified=80"),
        tmp.path(),
        30,
    );
    assert_ne!(diff_exit, 0, "full git diff must fail: {diff_err}");
    // show_changes(include_diff=true) must report failure: the diff filter
    // success must not mask the upstream git diff failure. Run the bounded
    // command with the same failing external diff in the environment so the
    // full `git diff` inside the command also fails.
    let cmd = show_changes_command(true, 20, 80);
    // Export the failing external diff into the environment so the full
    // `git diff` inside the bounded production script inherits it. The prefix
    // must be `export ...;` rather than an inline `VAR=... <cmd>` assignment:
    // the production script starts with a brace group `{ ... }`, and a
    // variable-assignment prefix is only valid before a *simple* command, not
    // a compound command (POSIX sh reports `Syntax error: "}" unexpected`).
    let (_exit, stdout, stderr) = run_command_full_capture(
        &format!("export GIT_EXTERNAL_DIFF={ext_diff:?}; {cmd}"),
        tmp.path(),
        30,
    );
    let stdout_bytes = stdout.len();
    assert!(
        stdout_bytes <= SHOW_CHANGES_OUTPUT_BUDGET_BYTES,
        "stdout {stdout_bytes}\n{stderr}"
    );
    let frames = split_show_changes_stdout(&stdout, true);
    // The structured metadata must carry the real full diff exit code (git
    // reports 128 when an external diff dies) and a command_failed diff_status.
    let diff_exit = frames.diff_exit.expect("diff_exit must be captured");
    assert_ne!(diff_exit, 0, "full git diff exit code must be non-zero");
    let output = bounded_show_changes_output_from_frames(&frames, tmp.path(), true, 20, 80, "");
    assert_eq!(output["diff_exit"], diff_exit);
    assert_eq!(output["diff_status"]["status"], "command_failed");
    assert_eq!(output["diff_status"]["exit_code"], diff_exit);
    // diff --stat succeeded (it does not invoke the external diff); its exit
    // must be reported as 0.
    assert_eq!(frames.diff_stat_exit, Some(0));
    assert_eq!(output["diff_stat_exit"], 0);
    assert_show_changes_envelope_value_matches_schema(&output, "diff exit propagation");
    // The structured diff_status already reports the failure; the runtime path
    // (separate test) asserts the tool result itself is not successful.
}

#[test]
fn show_changes_long_commit_subject_stays_within_budget() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    // A commit subject far longer than any single frame; the HEAD metadata
    // segment is bounded by its own byte budget by the production script.
    let subject = "Z".repeat(40_000);
    commit_file(tmp.path(), "README.md", "hello\n", &subject);
    std::fs::write(tmp.path().join("README.md"), "hello\nchanged\n").unwrap();
    let command = show_changes_command(false, 20, 80);
    assert!(
        command.contains("dd bs=1 count=$((head_subject_limit+1))"),
        "HEAD producer must read at most budget+1 bytes: {command}"
    );
    assert!(!command.contains("while IFS= read -r hline"));
    assert!(!command.contains("head_buf="));
    let (stdout_bytes, stdout, stderr) = run_bounded_show_changes_full(tmp.path(), false, 20, 80);
    assert!(
        stdout_bytes <= SHOW_CHANGES_OUTPUT_BUDGET_BYTES,
        "long-subject stdout must stay within budget; got {stdout_bytes}\n{stdout}\n{stderr}"
    );
    assert!(
        !stdout.contains(&subject),
        "the complete oversized subject must never reach the accumulated stdout"
    );
    let frames = split_show_changes_stdout(&stdout, false);
    let observation =
        parse_show_changes_status_observation(&frames.status, &frames.status_result, &stderr);
    assert!(observation.status_observed());
    // The pathological subject overflows the HEAD byte budget, so the HEAD
    // record is dropped whole and reported via the structured truncation flag
    // and reason code rather than leaking into the output.
    assert_eq!(frames.head_truncated, Some(true));
    assert!(frames.head.is_empty(), "HEAD record must be dropped whole");
    assert_eq!(frames.head_bytes, Some(0));
    let output =
        bounded_show_changes_output_from_frames(&frames, tmp.path(), false, 20, 80, &stderr);
    let reasons = output["truncation_reasons"].as_array().unwrap();
    assert!(
        reasons.iter().any(|r| r == "head_metadata_byte_budget"),
        "expected head_metadata_byte_budget reason: {reasons:?}"
    );
    assert_eq!(output["head"]["commit"], Value::Null);
    assert_eq!(output["transport_safe"], true);
    assert_show_changes_envelope_value_matches_schema(&output, "long subject");
}

#[cfg(unix)]
#[tokio::test]
async fn show_changes_runtime_rejects_stat_only_failure_for_both_diff_modes() {
    use std::os::unix::fs::PermissionsExt;

    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    commit_file(repo.path(), "README.md", "before\n", "initial");
    std::fs::write(repo.path().join("README.md"), "after\n").unwrap();

    let (git_lookup_exit, real_git, git_lookup_stderr) =
        run_command_full_capture("command -v git", repo.path(), 30);
    assert_eq!(git_lookup_exit, 0, "cannot locate git: {git_lookup_stderr}");
    let real_git = real_git.trim();
    assert!(!real_git.is_empty());

    let wrapper_dir = crate::test_support::executable_tempdir();
    let wrapper_path = wrapper_dir.path().join("git");
    std::fs::write(
        &wrapper_path,
        format!(
            "#!/bin/sh\nif [ \"$1\" = diff ] && [ \"$2\" = --stat ]; then\n  exit 77\nfi\nexec {} \"$@\"\n",
            shell_single_quote(real_git)
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&wrapper_path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&wrapper_path, permissions).unwrap();

    let runtime = test_runtime();
    let project = register_runner_project_at_path(&runtime, "stat-only", "demo", repo.path()).await;
    let path_prefix = format!(
        "PATH={}:\"$PATH\"; export PATH;",
        shell_single_quote(wrapper_dir.path().to_str().unwrap())
    );

    for include_diff in [false, true] {
        let task = tokio::spawn({
            let runtime = runtime.clone();
            let project = project.clone();
            async move {
                runtime
                    .show_changes(project, None, Some(include_diff), None, None, None)
                    .await
            }
        });
        let req = wait_for_patch_agent_request(&runtime, "stat-only").await;
        assert_eq!(req.kind, "run_internal_posix_script");
        assert!(req.command.is_empty());
        let payload = req
            .script
            .as_ref()
            .expect("read_workspace_changes must carry a typed internal script");
        assert_eq!(
            payload.language,
            crate::runner_protocol::ShellScriptLanguage::Sh
        );
        assert!(payload.args.is_empty());
        assert!(
            payload.script.len() <= crate::runner_protocol::RAW_SHELL_WIRE_MAX_BYTES,
            "script bytes={}",
            payload.script.len()
        );
        let full_command = format!("{path_prefix} {}", payload.script);
        let (command_exit, stdout, stderr) =
            run_command_full_capture(&full_command, repo.path(), 30);
        assert_eq!(
            command_exit, 0,
            "bounded envelope failed: {stderr}\n{stdout}"
        );
        assert!(
            stdout.len() <= SHOW_CHANGES_OUTPUT_BUDGET_BYTES,
            "bounded stdout bytes={}",
            stdout.len()
        );

        let frames = split_show_changes_stdout(&stdout, include_diff);
        let status_exit = frames
            .status_result
            .lines()
            .find_map(|line| line.strip_prefix("status_exit="))
            .and_then(|value| value.parse::<i32>().ok());
        assert_eq!(status_exit, Some(0));
        assert_eq!(frames.diff_stat_exit, Some(77));
        if include_diff {
            assert_eq!(frames.diff_exit, Some(0));
        } else {
            assert_eq!(frames.diff_exit, None);
        }

        complete_patch_agent_request(
            &runtime,
            "stat-only",
            &req.request_id,
            command_exit,
            &stdout,
            &stderr,
        )
        .await;
        let result = task.await.unwrap();
        assert!(
            !result.success,
            "stat failure must fail ToolResult: {result:?}"
        );
        assert!(
            result
                .error
                .as_deref()
                .is_some_and(|error| error.contains("diff-stat inspection")),
            "unexpected error: {:?}",
            result.error
        );
        assert_eq!(result.output["diff_stat_exit"], 77);
        assert_eq!(
            result.output["diff_stat_status"]["status"],
            "command_failed"
        );
        assert_eq!(result.output["diff_stat_status"]["exit_code"], 77);
        assert_eq!(
            result.output["diff_stat_status"]["reason_code"],
            "git_diff_stat_command_failed"
        );
        assert_eq!(result.output["transport_safe"], true);
        assert_eq!(result.output["status_observation"]["status"], "observed");
        if include_diff {
            assert_eq!(result.output["diff_exit"], 0);
            assert_eq!(result.output["diff_status"]["status"], "observed");
        }
        assert_show_changes_envelope_matches_schema("runtime stat-only failure", &result);
        eprintln!(
            "stat_only include_diff={include_diff} command_exit={command_exit} status_exit={} diff_stat_exit={} diff_exit={:?} tool_success={} stdout_bytes={}",
            status_exit.unwrap(),
            frames.diff_stat_exit.unwrap(),
            frames.diff_exit,
            result.success,
            stdout.len()
        );
    }
}

#[tokio::test]
async fn show_changes_runtime_rejects_unavailable_diff_stat_observation() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    commit_file(repo.path(), "README.md", "before\n", "initial");
    std::fs::write(repo.path().join("README.md"), "after\n").unwrap();

    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "stat-missing", "demo", repo.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .show_changes(project, None, Some(false), None, None, None)
                .await
        }
    });
    let req = wait_for_patch_agent_request(&runtime, "stat-missing").await;
    assert_eq!(req.kind, "run_internal_posix_script");
    assert!(req.command.is_empty());
    let payload = req
        .script
        .as_ref()
        .expect("read_workspace_changes must carry a typed internal script");
    let (command_exit, stdout, stderr) = run_command_full_capture(&payload.script, repo.path(), 30);
    assert_eq!(
        command_exit, 0,
        "read_workspace_changes command failed: {stderr}"
    );
    let missing_stat_exit = stdout.replacen("diff_stat_exit=0", "xiff_stat_exit=0", 1);
    complete_patch_agent_request(
        &runtime,
        "stat-missing",
        &req.request_id,
        command_exit,
        &missing_stat_exit,
        &stderr,
    )
    .await;

    let result = task.await.unwrap();
    assert!(!result.success);
    assert!(
        result
            .error
            .as_deref()
            .is_some_and(|error| error.contains("diff-stat inspection unavailable")),
        "unexpected error: {:?}",
        result.error
    );
    assert_eq!(result.output["diff_stat_exit"], Value::Null);
    assert_eq!(
        result.output["diff_stat_status"],
        json!({
            "status": "output_unavailable",
            "exit_code": null,
            "reason_code": "git_diff_stat_result_unavailable",
        })
    );
    assert_show_changes_envelope_matches_schema("runtime unavailable stat", &result);
}

#[test]
fn show_changes_many_long_path_diff_stat_stays_within_budget() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    // Many tracked files with long names produce a large `git diff --stat`
    // segment; the production script bounds it.
    let dir = tmp.path().join("d");
    std::fs::create_dir_all(&dir).unwrap();
    let total = 220usize;
    for i in 0..total {
        std::fs::write(dir.join(long_leaf_name(i)), "x\n").unwrap();
    }
    let (add_exit, _, add_stderr, _) = run_command_sync("git add -A", tmp.path(), 30);
    assert_eq!(add_exit, 0, "add failed: {add_stderr}");
    let (commit_exit, _, commit_stderr, _) = run_command_sync("git commit -qm add", tmp.path(), 30);
    assert_eq!(commit_exit, 0, "commit failed: {commit_stderr}");
    for i in 0..total {
        std::fs::write(dir.join(long_leaf_name(i)), "y\nx\n").unwrap();
    }
    let (stdout_bytes, stdout, stderr) = run_bounded_show_changes_full(tmp.path(), false, 20, 80);
    assert!(
        stdout_bytes <= SHOW_CHANGES_OUTPUT_BUDGET_BYTES,
        "many-long-path diff --stat stdout must stay within budget; got {stdout_bytes}\n{stderr}"
    );
    let frames = split_show_changes_stdout(&stdout, false);
    // The diff --stat exit code must be reported (git diff --stat succeeds).
    assert_eq!(frames.diff_stat_exit, Some(0));
    let output =
        bounded_show_changes_output_from_frames(&frames, tmp.path(), false, 20, 80, &stderr);
    assert_show_changes_envelope_value_matches_schema(&output, "many long path diff stat");
}

#[test]
fn show_changes_single_overlong_diff_line_stays_within_budget() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let original = "a\n".to_string();
    commit_file(tmp.path(), "big.txt", &original, "initial");
    // One diff line far larger than the diff byte budget; a single overlong
    // line must not overflow the global budget.
    let giant = "X".repeat(60_000);
    std::fs::write(tmp.path().join("big.txt"), format!("a\n{giant}\n")).unwrap();
    let (stdout_bytes, stdout, stderr) = run_bounded_show_changes_full(tmp.path(), true, 20, 80);
    assert!(
        stdout_bytes <= SHOW_CHANGES_OUTPUT_BUDGET_BYTES,
        "overlong-diff-line stdout must stay within budget; got {stdout_bytes}\n{stdout}\n{stderr}"
    );
    let frames = split_show_changes_stdout(&stdout, true);
    assert_eq!(frames.diff_exit, Some(0));
    assert_eq!(frames.diff_hunks_truncated, Some(true));
    assert_eq!(frames.diff_trunc_bytes, Some(true));
    assert_eq!(frames.diff_trunc_bytes_in_hunk, Some(true));
    let output =
        bounded_show_changes_output_from_frames(&frames, tmp.path(), true, 20, 80, &stderr);
    let reasons = output["truncation_reasons"].as_array().unwrap();
    assert!(
        reasons.iter().any(|r| matches!(
            r.as_str(),
            Some("diff_hunk_line_limit") | Some("diff_byte_budget")
        )),
        "expected a diff line/byte budget reason: {reasons:?}"
    );
    assert!(reasons.iter().any(|r| r == "diff_hunk_byte_budget"));
    assert_git_diff_hunks_recovery_call_parses(&output["diff_review_handoff"]["next_call"]);
    assert_show_changes_envelope_value_matches_schema(&output, "overlong diff line");
    // The giant line must not appear in full in the structured output.
    let serialized = serde_json::to_string(&output).unwrap();
    assert!(
        !serialized.contains(&giant),
        "a single overlong diff line must not leak into the structured output"
    );
}

#[test]
fn show_changes_binary_diff_stays_within_budget() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "bin.dat", "text\n", "initial");
    std::fs::write(tmp.path().join("bin.dat"), vec![0, 1, 2, 3, 0, 4]).unwrap();
    let (stdout_bytes, stdout, stderr) = run_bounded_show_changes_full(tmp.path(), true, 20, 80);
    assert!(
        stdout_bytes <= SHOW_CHANGES_OUTPUT_BUDGET_BYTES,
        "binary diff stdout must stay within budget; got {stdout_bytes}\n{stderr}"
    );
    let frames = split_show_changes_stdout(&stdout, true);
    assert_eq!(frames.diff_exit, Some(0));
    let output =
        bounded_show_changes_output_from_frames(&frames, tmp.path(), true, 20, 80, &stderr);
    // A binary diff produces no hunks but must not crash or leak binary bytes.
    assert_eq!(output["hunk_count"].as_u64().unwrap_or(0), 0);
    assert_show_changes_envelope_value_matches_schema(&output, "binary diff");
}

#[test]
fn show_changes_multi_file_multi_hunk_diff_stays_within_budget() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    // 5 files, 3 hunks each, all selected under max_hunks=20.
    for i in 0..5 {
        let content = (0..30).map(|n| format!("line{n}\n")).collect::<String>();
        commit_file(tmp.path(), &format!("f{i}.txt"), &content, "init");
    }
    for i in 0..5 {
        let path = tmp.path().join(format!("f{i}.txt"));
        let content = (0..30)
            .map(|n| {
                if n % 10 == 0 {
                    format!("mod{n}\n")
                } else {
                    format!("line{n}\n")
                }
            })
            .collect::<String>();
        std::fs::write(path, content).unwrap();
    }
    let (stdout_bytes, stdout, stderr) = run_bounded_show_changes_full(tmp.path(), true, 20, 80);
    assert!(
        stdout_bytes <= SHOW_CHANGES_OUTPUT_BUDGET_BYTES,
        "multi-file multi-hunk stdout must stay within budget; got {stdout_bytes}\n{stderr}"
    );
    let frames = split_show_changes_stdout(&stdout, true);
    assert_eq!(frames.diff_exit, Some(0));
    let output =
        bounded_show_changes_output_from_frames(&frames, tmp.path(), true, 20, 80, &stderr);
    // 5 files, ~15 hunks selected (some may be line-bounded).
    let hunk_count = output["hunk_count"].as_u64().unwrap_or(0);
    assert!(hunk_count >= 5, "expected several hunks, got {hunk_count}");
    assert_show_changes_envelope_value_matches_schema(&output, "multi-file multi-hunk");
}

#[test]
fn show_changes_include_diff_false_stays_within_budget() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    std::fs::write(tmp.path().join("README.md"), "hello\nchanged\n").unwrap();
    for i in 0..10 {
        std::fs::write(tmp.path().join(format!("u{i}.txt")), "x\n").unwrap();
    }
    let (stdout_bytes, _stdout, _stderr) = run_bounded_show_changes_full(tmp.path(), false, 20, 80);
    assert!(
        stdout_bytes <= SHOW_CHANGES_OUTPUT_BUDGET_BYTES,
        "include_diff=false stdout must stay within budget; got {stdout_bytes}"
    );
}

#[test]
fn show_changes_include_diff_true_stays_within_budget() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    std::fs::write(tmp.path().join("README.md"), "hello\nchanged\n").unwrap();
    let (stdout_bytes, _stdout, _stderr) = run_bounded_show_changes_full(tmp.path(), true, 20, 80);
    assert!(
        stdout_bytes <= SHOW_CHANGES_OUTPUT_BUDGET_BYTES,
        "include_diff=true stdout must stay within budget; got {stdout_bytes}"
    );
}

#[test]
fn show_changes_status_command_failure_stays_within_budget_and_fails() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    let (cfg_exit, _, cfg_err) = run_command_full_capture(
        "git config status.showUntrackedFiles invalid",
        tmp.path(),
        30,
    );
    assert_eq!(cfg_exit, 0, "config failed: {cfg_err}");
    let cmd = show_changes_command(false, 20, 80);
    let (exit, stdout, stderr) = run_command_full_capture(&cmd, tmp.path(), 30);
    assert_ne!(exit, 0, "status config error must not exit 0: {stderr}");
    assert!(
        stdout.len() <= SHOW_CHANGES_OUTPUT_BUDGET_BYTES,
        "status-failure stdout must stay within budget; got {}",
        stdout.len()
    );
    let frames = split_show_changes_stdout(&stdout, false);
    let observation =
        parse_show_changes_status_observation(&frames.status, &frames.status_result, &stderr);
    assert_eq!(observation.as_json()["status"], "command_failed");
    let output =
        bounded_show_changes_output_from_frames(&frames, tmp.path(), false, 20, 80, &stderr);
    assert_eq!(output["clean"], Value::Null);
    assert_eq!(output["counts"]["conflicted"], Value::Null);
    assert_eq!(output["verdict"]["status"], "fail");
    assert_show_changes_envelope_value_matches_schema(&output, "status command failure");
}

#[tokio::test]
async fn show_changes_runtime_propagates_full_diff_failure_as_tool_failure() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    std::fs::write(tmp.path().join("README.md"), "hello\nchanged\n").unwrap();
    // Install a failing external diff that affects the full `git diff` only.
    let marker = tmp.path().join("extdiff_runtime_ran.txt");
    let marker_str = marker.to_str().unwrap();
    let ext_diff = format!(
        "sh -c 'printf x > {marker_str}; exit 5' \"$1\" \"$2\" \"$3\" \"$4\" \"$5\" \"$6\" \"$7\""
    );
    let runtime = test_runtime();
    let project = register_runner_project_at_path(&runtime, "extd", "demo", tmp.path()).await;
    // The agent completes the request by running the bounded command locally,
    // but with the failing external diff exported into the environment. The
    // production script starts with a brace group, so the external diff must
    // be `export`ed (with a `;` separator): an inline `VAR=... <cmd>` prefix
    // is only valid before a simple command, not the compound `{ ... }`.
    let ext_env = format!("export GIT_EXTERNAL_DIFF={ext_diff:?};");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .show_changes(project, None, Some(true), None, None, None)
                .await
        }
    });
    let req = wait_for_patch_agent_request(&runtime, "extd").await;
    // Run the generated internal script locally with the failing external diff
    // in the environment so the full `git diff` fails.
    assert_eq!(req.kind, "run_internal_posix_script");
    assert!(req.command.is_empty());
    let payload = req
        .script
        .as_ref()
        .expect("read_workspace_changes must carry a typed internal script");
    let full = format!("{ext_env} {}", payload.script);
    let (exit, stdout, stderr) = run_command_full_capture(&full, tmp.path(), 30);
    complete_patch_agent_request(&runtime, "extd", &req.request_id, exit, &stdout, &stderr).await;
    let result = task.await.unwrap();
    // The full git diff failed, so show_changes must report a tool failure even
    // though the production-side diff filter succeeded.
    assert!(
        !result.success,
        "read_workspace_changes must fail when the full git diff fails: {:?}",
        result.error
    );
    assert_eq!(result.output["diff_status"]["status"], "command_failed");
    let diff_exit = result.output["diff_status"]["exit_code"]
        .as_i64()
        .expect("diff exit code");
    assert_ne!(diff_exit, 0, "full git diff exit code must be non-zero");
    assert_eq!(result.output["diff_exit"], diff_exit);
    // diff --stat itself succeeded (it does not invoke the external diff); its
    // exit must be reported as 0.
    assert_eq!(result.output["diff_stat_exit"], 0);
    assert_eq!(result.output["status_observation"]["status"], "observed");
    assert_show_changes_envelope_matches_schema("runtime diff failure", &result);
}

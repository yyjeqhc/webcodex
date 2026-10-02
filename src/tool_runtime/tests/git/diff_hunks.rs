#[test]
fn git_diff_hunks_tool_is_known_and_schema_is_bounded() {
    assert!(is_known_tool_name("read_git_diff_hunks"));
    let call = ToolCall::from_tool_name(
        "read_git_diff_hunks",
        json!({
            "project":"agent:oe:webcodex",
            "paths":["src/runtime_http.rs"],
            "max_hunks":20,
            "max_hunk_lines":120,
            "max_page_bytes":98304,
            "cached":true,
            "continuation":"opaque-continuation"
        }),
    )
    .unwrap();
    assert!(matches!(
        call,
        ToolCall::GitDiffHunks {
            project,
            cached: Some(true),
            max_page_bytes: Some(98304),
            continuation: Some(continuation),
            ..
        } if project == "agent:oe:webcodex" && continuation == "opaque-continuation"
    ));

    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "read_git_diff_hunks");
    let props = spec.input_schema["properties"].as_object().unwrap();
    for field in [
        "project",
        "paths",
        "max_hunks",
        "max_hunk_lines",
        "max_page_bytes",
        "cached",
        "base_commit",
        "head_commit",
        "continuation",
    ] {
        assert!(props.contains_key(field), "missing {}", field);
    }
    assert_eq!(
        props["continuation"]["maxLength"],
        GIT_DIFF_HUNKS_CONTINUATION_MAX_BYTES
    );
    assert_eq!(props["max_page_bytes"]["minimum"], 0);
    assert_eq!(
        props["max_page_bytes"]["default"],
        DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES
    );
    assert!(props["max_page_bytes"].get("maximum").is_none());
    assert!(props["max_page_bytes"]["description"]
        .as_str()
        .unwrap()
        .contains("runtime-clamped"));
    for field in ["base_commit", "head_commit"] {
        assert_eq!(props[field]["minLength"], 40);
        assert_eq!(props[field]["maxLength"], 40);
        assert_eq!(props[field]["pattern"], "^[0-9A-Fa-f]{40}$");
    }
    assert!(spec.input_schema.get("allOf").is_none());
    let committed_call = ToolCall::from_tool_name(
        "read_git_diff_hunks",
        json!({
            "project": "agent:oe:webcodex",
            "base_commit": "A".repeat(40),
            "head_commit": "b".repeat(40),
            "paths": ["src/runtime_http.rs"]
        }),
    )
    .unwrap();
    assert!(matches!(
        committed_call,
        ToolCall::GitDiffHunks {
            base_commit: Some(base),
            head_commit: Some(head),
            cached: None,
            ..
        } if base == "A".repeat(40) && head == "b".repeat(40)
    ));
    let committed_false_call = ToolCall::from_tool_name(
        "read_git_diff_hunks",
        json!({
            "project": "agent:oe:webcodex",
            "base_commit": "A".repeat(40),
            "head_commit": "b".repeat(40),
            "cached": false,
        }),
    )
    .unwrap();
    assert!(matches!(
        committed_false_call,
        ToolCall::GitDiffHunks {
            base_commit: Some(_),
            head_commit: Some(_),
            cached: Some(false),
            ..
        }
    ));
    assert!(ToolCall::from_tool_name(
        "read_git_diff_hunks",
        json!({
            "project": "agent:oe:webcodex",
            "continuation": "x".repeat(GIT_DIFF_HUNKS_CONTINUATION_MAX_BYTES + 1),
        }),
    )
    .is_err());
    let output_props = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    for field in [
        "project",
        "paths",
        "cached",
        "files",
        "max_page_bytes",
        "hunk_count",
        "truncated",
        "truncation_reasons",
        "has_more",
        "recovery",
        "exit_code",
        "stderr",
    ] {
        assert!(output_props.contains_key(field), "missing {}", field);
    }
    let recovery = &output_props["recovery"];
    assert!(output_props.get("next_continuation").is_none());
    assert!(recovery["properties"]["later_hunks"].is_object());
    let current_hunk = &recovery["properties"]["current_hunk"];
    for field in ["reason_code", "next_call"] {
        assert!(current_hunk["properties"].get(field).is_some());
    }
}

#[test]
fn git_diff_hunks_session_audit_redacts_continuation() {
    let continuation = "WCDH_UNIQUE_AUDIT_CONTINUATION_7f18b4a2";
    let arguments = json!({
        "project": "agent:oe:webcodex",
        "paths": ["src/runtime_http.rs", "src/tool_runtime/git.rs"],
        "max_hunks": 7,
        "max_hunk_lines": 33,
        "max_page_bytes": 96 * 1024,
        "cached": true,
        "continuation": continuation,
    });

    let raw_summary = super::super::tool_audit::session_log_arguments_for_tool_request(
        "read_git_diff_hunks",
        &arguments,
    );
    assert_eq!(raw_summary["project"], "agent:oe:webcodex");
    assert_eq!(
        raw_summary["paths"],
        json!(["src/runtime_http.rs", "src/tool_runtime/git.rs"])
    );
    assert_eq!(raw_summary["max_hunks"], 7);
    assert_eq!(raw_summary["max_hunk_lines"], 33);
    assert_eq!(raw_summary["max_page_bytes"], 96 * 1024);
    assert_eq!(raw_summary["cached"], true);
    assert_eq!(raw_summary["continuation_present"], true);
    assert!(raw_summary.get("continuation").is_none());
    assert!(!serde_json::to_string(&raw_summary)
        .unwrap()
        .contains(continuation));

    let call = ToolCall::from_tool_name("read_git_diff_hunks", arguments.clone()).unwrap();
    let typed_summary = call.session_log_arguments();
    assert_eq!(typed_summary["project"], "agent:oe:webcodex");
    assert_eq!(typed_summary["paths"], raw_summary["paths"]);
    assert_eq!(typed_summary["max_hunks"], 7);
    assert_eq!(typed_summary["max_hunk_lines"], 33);
    assert_eq!(typed_summary["max_page_bytes"], 96 * 1024);
    assert_eq!(typed_summary["cached"], true);
    assert!(typed_summary.get("continuation").is_none());
    assert!(!serde_json::to_string(&typed_summary)
        .unwrap()
        .contains(continuation));

    let defensive =
        super::super::sessions::session_input_summary_for_tool("read_git_diff_hunks", &arguments);
    assert_eq!(defensive["project"], "agent:oe:webcodex");
    assert_eq!(defensive["paths"], raw_summary["paths"]);
    assert_eq!(defensive["max_hunks"], 7);
    assert_eq!(defensive["max_hunk_lines"], 33);
    assert_eq!(defensive["max_page_bytes"], 96 * 1024);
    assert_eq!(defensive["cached"], true);
    assert!(defensive.get("continuation").is_none());
    assert!(!serde_json::to_string(&defensive)
        .unwrap()
        .contains(continuation));

    let runtime = test_runtime();
    let session = runtime.sessions.start_session(
        Some("agent:oe:webcodex".to_string()),
        Some("git diff audit".to_string()),
    );
    runtime.sessions.record_tool_call_started(
        Some(&session.session_id),
        crate::tool_runtime::sessions::SessionTransport::Api,
        "read_git_diff_hunks",
        &arguments,
        crate::tool_runtime::sessions::session_tool_contract("read_git_diff_hunks"),
    );
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(10))
        .unwrap();
    let input_summary = summary.events[0].input_summary.as_ref().unwrap();
    assert_eq!(input_summary["project"], "agent:oe:webcodex");
    assert_eq!(input_summary["paths"], raw_summary["paths"]);
    assert_eq!(input_summary["max_hunks"], 7);
    assert_eq!(input_summary["max_hunk_lines"], 33);
    assert_eq!(input_summary["max_page_bytes"], 96 * 1024);
    assert_eq!(input_summary["cached"], true);
    assert!(input_summary.get("continuation").is_none());
    assert!(!serde_json::to_string(input_summary)
        .unwrap()
        .contains(continuation));

    let base = "A".repeat(40);
    let head = "b".repeat(40);
    let committed_arguments = json!({
        "project": "agent:oe:webcodex",
        "paths": ["src/runtime_http.rs"],
        "base_commit": base,
        "head_commit": head,
        "continuation": continuation,
    });
    let committed_summary = super::super::tool_audit::session_log_arguments_for_tool_request(
        "read_git_diff_hunks",
        &committed_arguments,
    );
    assert_eq!(committed_summary["base_commit"], "a".repeat(40));
    assert_eq!(committed_summary["head_commit"], "b".repeat(40));
    assert_eq!(committed_summary["base_commit_valid"], true);
    assert_eq!(committed_summary["head_commit_valid"], true);
    assert!(committed_summary.get("continuation").is_none());

    let private_hunk = "PRIVATE_GIT_DIFF_HUNK_BODY_91e2";
    let result_summary = super::super::tool_audit::session_log_result_for_tool(
        "read_git_diff_hunks",
        &json!({
            "project": "agent:oe:webcodex",
            "scope": {
                "mode": "committed",
                "requested_base": "a".repeat(40),
                "requested_head": "b".repeat(40),
                "merge_base": "a".repeat(40),
                "base_is_ancestor": true,
                "diff_range": format!("{}..{}", "a".repeat(40), "b".repeat(40))
            },
            "cached": false,
            "files": [{"path": "src/runtime_http.rs", "hunks": [{"diff": private_hunk}]}],
            "hunk_count": 1,
            "truncated": true,
            "truncation_reasons": ["page_hunk_limit"],
            "has_more": true,
            "next_continuation": continuation,
            "exit_code": 0,
            "stderr": "PRIVATE_STDERR",
        }),
    );
    let serialized_result = serde_json::to_string(&result_summary).unwrap();
    assert_eq!(result_summary["hunk_count"], 1);
    assert_eq!(result_summary["file_count"], 1);
    assert!(result_summary.get("files").is_none());
    assert!(result_summary.get("next_continuation").is_none());
    assert!(result_summary.get("stderr").is_none());
    assert!(!serialized_result.contains(private_hunk));
    assert!(!serialized_result.contains(continuation));
    assert!(!serialized_result.contains("PRIVATE_STDERR"));
}

#[test]
fn show_changes_tool_is_known_and_parses() {
    assert!(is_known_tool_name("read_workspace_changes"));
    let call = ToolCall::from_tool_name(
        "read_workspace_changes",
        json!({
            "project": "agent:oe:webcodex",
            "include_diff": true,
            "max_hunks": 4,
            "max_hunk_lines": 12,
            "session_id": "wc_sess_1234",
            "session_event_limit": 8
        }),
    )
    .unwrap();
    assert!(matches!(
        call,
        ToolCall::ShowChanges {
            project,
            session_id: Some(session_id),
            include_diff: Some(true),
            max_hunks: Some(4),
            max_hunk_lines: Some(12),
            session_event_limit: Some(8)
        } if project == "agent:oe:webcodex" && session_id == "wc_sess_1234"
    ));

    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "read_workspace_changes");
    let output_props = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert!(
        output_props.contains_key("verdict"),
        "read_workspace_changes output schema should expose verdict"
    );
    assert!(
        output_props.contains_key("diff_stat_status"),
        "read_workspace_changes output schema should expose strict diff-stat observation"
    );
}

async fn run_runner_git_diff_hunks_page(
    runtime: &ToolRuntime,
    client_id: &str,
    project: &str,
    repo: &Path,
    paths: Option<Vec<String>>,
    max_hunks: usize,
    max_hunk_lines: usize,
    cached: bool,
    continuation: Option<String>,
) -> (ToolResult, usize, String) {
    run_runner_git_diff_hunks_page_with_budget(
        runtime,
        client_id,
        project,
        repo,
        paths,
        max_hunks,
        max_hunk_lines,
        None,
        cached,
        continuation,
    )
    .await
}

async fn run_runner_git_diff_hunks_page_with_budget(
    runtime: &ToolRuntime,
    client_id: &str,
    project: &str,
    repo: &Path,
    paths: Option<Vec<String>>,
    max_hunks: usize,
    max_hunk_lines: usize,
    max_page_bytes: Option<usize>,
    cached: bool,
    continuation: Option<String>,
) -> (ToolResult, usize, String) {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.to_string();
        async move {
            runtime
                .git_diff_hunks_continued_with_range_and_page_bytes(
                    project,
                    paths,
                    Some(max_hunks),
                    Some(max_hunk_lines),
                    max_page_bytes,
                    Some(cached),
                    None,
                    None,
                    continuation,
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(runtime, client_id).await;
    assert_eq!(request.kind, "run_internal_posix_script");
    assert_eq!(
        request.cwd.as_deref(),
        Some(repo.to_string_lossy().as_ref())
    );
    assert!(request.command.is_empty());
    let script = request
        .script
        .as_ref()
        .expect("read_git_diff_hunks must carry a typed internal script")
        .script
        .clone();
    let (exit_code, stdout, stderr) = run_runner_shell_request_locally(&request);
    let stdout_bytes = stdout.len();
    complete_patch_agent_request(
        runtime,
        client_id,
        &request.request_id,
        exit_code,
        &stdout,
        &stderr,
    )
    .await;
    (task.await.unwrap(), stdout_bytes, script)
}

async fn run_git_diff_hunks_with_faulted_source(
    client_id: &str,
    mutate: impl FnOnce(i32, String, String) -> (i32, String, String, bool, bool),
) -> ToolResult {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    write_git_review_fixture_file(repo.path(), "src/a.rs", "pub fn a() -> u8 { 1 }\n");
    commit_git_review_fixture(repo.path(), "base");
    write_git_review_fixture_file(repo.path(), "src/a.rs", "pub fn a() -> u8 { 2 }\n");

    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, client_id, "repo", repo.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .git_diff_hunks_continued_with_range_and_page_bytes(
                    project,
                    Some(vec!["src/a.rs".to_string()]),
                    Some(10),
                    Some(120),
                    None,
                    Some(false),
                    None,
                    None,
                    None,
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    let (exit_code, stdout, stderr) = run_runner_shell_request_locally(&request);
    let (exit_code, stdout, stderr, stdout_truncated, stderr_truncated) =
        mutate(exit_code, stdout, stderr);
    complete_patch_agent_request_with_truncation(
        &runtime,
        client_id,
        &request.request_id,
        exit_code,
        &stdout,
        &stderr,
        stdout_truncated,
        stderr_truncated,
    )
    .await;
    task.await.unwrap()
}

#[tokio::test]
async fn git_diff_hunks_source_failures_report_diagnostic_stage() {
    let truncated =
        run_git_diff_hunks_with_faulted_source("git-source-truncated", |exit, stdout, stderr| {
            (exit, stdout, stderr, true, false)
        })
        .await;
    assert_eq!(truncated.output["reason_code"], "source_output_truncated");
    assert_eq!(truncated.output["source_stage"], "runner_output");
    assert_eq!(truncated.output["stdout_truncated"], true);
    assert!(truncated.output.get("stdout").is_none());

    let malformed = run_git_diff_hunks_with_faulted_source(
        "git-source-frame-invalid",
        |_exit, _stdout, stderr| (0, "not-a-wcdh-frame".to_string(), stderr, false, false),
    )
    .await;
    assert_eq!(malformed.output["reason_code"], "source_frame_invalid");
    assert_eq!(malformed.output["source_stage"], "frame_parse");
    assert_eq!(malformed.output["exit_code"], 0);

    let execution = run_git_diff_hunks_with_faulted_source(
        "git-source-execution-failed",
        |_exit, _stdout, stderr| (2, String::new(), stderr, false, false),
    )
    .await;
    assert_eq!(execution.output["reason_code"], "source_execution_failed");
    assert_eq!(execution.output["source_stage"], "source_execution");

    let fence = run_git_diff_hunks_with_faulted_source(
        "git-source-fence-invalid",
        |_exit, stdout, stderr| {
            let mutated = stdout
                .replacen("pre_hash_exit=0", "pre_hash_exit=1", 1)
                .replacen("stale=0", "stale=1", 1);
            assert_ne!(mutated, stdout);
            // A failed fence observation can also make an expected-fence comparison
            // look stale. Fence validity is authoritative before stale identity.
            (1, mutated, stderr, false, false)
        },
    )
    .await;
    assert_eq!(fence.output["reason_code"], "source_fence_unavailable");
    assert_eq!(fence.output["source_stage"], "source_fence");

    let source_changed = run_git_diff_hunks_with_faulted_source(
        "git-source-changed",
        |_exit, mut stdout, stderr| {
            let start = stdout.find("post_fence=").unwrap() + "post_fence=".len();
            let current = stdout.as_bytes()[start];
            let replacement = if current == b'a' { "b" } else { "a" };
            stdout.replace_range(start..start + 1, replacement);
            // pre_fence != post_fence also makes the generated script exit 1.
            (1, stdout, stderr, false, false)
        },
    )
    .await;
    assert_eq!(
        source_changed.output["reason_code"],
        "source_changed_during_observation"
    );

    let page_filter = run_git_diff_hunks_with_faulted_source(
        "git-source-page-filter",
        |_exit, stdout, stderr| {
            let mutated = stdout.replacen("page_filter_exit=0", "page_filter_exit=1", 1);
            assert_ne!(mutated, stdout);
            // A nonzero page filter exit is reflected by wrapper exit 1 too.
            (1, mutated, stderr, false, false)
        },
    )
    .await;
    assert_eq!(
        page_filter.output["reason_code"],
        "source_page_filter_failed"
    );
    assert_eq!(page_filter.output["source_stage"], "page_filter");

    let projection =
        run_git_diff_hunks_with_faulted_source("git-source-projection", |exit, stdout, stderr| {
            let mutated = stdout.replacen("returned_hunks=1", "returned_hunks=2", 1);
            assert_ne!(mutated, stdout);
            (exit, mutated, stderr, false, false)
        })
        .await;
    assert_eq!(
        projection.output["reason_code"],
        "source_projection_inconsistent"
    );
    assert_eq!(projection.output["source_stage"], "projection");
}

async fn run_parser_ready_worktree_git_diff_hunks_call(
    runtime: &ToolRuntime,
    client_id: &str,
    repo: &Path,
    call: &Value,
) -> ToolResult {
    let tool = call["tool"]
        .as_str()
        .expect("recovery call tool must be a string");
    let parsed = ToolCall::from_tool_name(tool, call["arguments"].clone())
        .expect("recovery next_call must parse directly");
    let ToolCall::GitDiffHunks {
        project,
        paths,
        max_hunks,
        max_hunk_lines,
        cached,
        base_commit,
        max_page_bytes,
        head_commit,
        continuation,
        ..
    } = parsed
    else {
        panic!("recovery next_call must target read_git_diff_hunks");
    };
    assert!(base_commit.is_none() && head_commit.is_none());
    let (result, _, _) = run_runner_git_diff_hunks_page_with_budget(
        runtime,
        client_id,
        &project,
        repo,
        paths,
        max_hunks.expect("recovery call must preserve max_hunks"),
        max_hunk_lines.expect("recovery call must preserve max_hunk_lines"),
        max_page_bytes,
        cached.unwrap_or(false),
        continuation,
    )
    .await;
    result
}

async fn run_parser_ready_committed_git_diff_hunks_call(
    runtime: &ToolRuntime,
    client_id: &str,
    repo: &Path,
    call: &Value,
) -> ToolResult {
    let tool = call["tool"]
        .as_str()
        .expect("recovery call tool must be a string");
    let parsed = ToolCall::from_tool_name(tool, call["arguments"].clone())
        .expect("committed recovery next_call must parse directly");
    let ToolCall::GitDiffHunks {
        project,
        paths,
        max_hunks,
        max_hunk_lines,
        cached,
        base_commit,
        max_page_bytes,
        head_commit,
        continuation,
        ..
    } = parsed
    else {
        panic!("recovery next_call must target read_git_diff_hunks");
    };
    assert!(
        cached.is_none(),
        "committed recovery must not project cached"
    );
    let (result, _, _) = run_runner_git_diff_hunks_committed_page_with_budget(
        runtime,
        client_id,
        &project,
        repo,
        paths,
        max_hunks.expect("recovery call must preserve max_hunks"),
        max_hunk_lines.expect("recovery call must preserve max_hunk_lines"),
        max_page_bytes,
        base_commit.expect("committed recovery must preserve base_commit"),
        head_commit.expect("committed recovery must preserve head_commit"),
        continuation,
    )
    .await;
    result
}

async fn run_runner_git_diff_hunks_committed_page(
    runtime: &ToolRuntime,
    client_id: &str,
    project: &str,
    repo: &Path,
    paths: Option<Vec<String>>,
    max_hunks: usize,
    max_hunk_lines: usize,
    base_commit: String,
    head_commit: String,
    continuation: Option<String>,
) -> (ToolResult, usize, Vec<String>) {
    run_runner_git_diff_hunks_committed_page_with_budget(
        runtime,
        client_id,
        project,
        repo,
        paths,
        max_hunks,
        max_hunk_lines,
        None,
        base_commit,
        head_commit,
        continuation,
    )
    .await
}

async fn run_runner_git_diff_hunks_committed_page_with_budget(
    runtime: &ToolRuntime,
    client_id: &str,
    project: &str,
    repo: &Path,
    paths: Option<Vec<String>>,
    max_hunks: usize,
    max_hunk_lines: usize,
    max_page_bytes: Option<usize>,
    base_commit: String,
    head_commit: String,
    continuation: Option<String>,
) -> (ToolResult, usize, Vec<String>) {
    run_runner_git_diff_hunks_committed_page_with_options(
        runtime,
        client_id,
        project,
        repo,
        paths,
        max_hunks,
        max_hunk_lines,
        max_page_bytes,
        None,
        base_commit,
        head_commit,
        continuation,
    )
    .await
}

async fn run_runner_git_diff_hunks_committed_page_with_options(
    runtime: &ToolRuntime,
    client_id: &str,
    project: &str,
    repo: &Path,
    paths: Option<Vec<String>>,
    max_hunks: usize,
    max_hunk_lines: usize,
    max_page_bytes: Option<usize>,
    cached: Option<bool>,
    base_commit: String,
    head_commit: String,
    continuation: Option<String>,
) -> (ToolResult, usize, Vec<String>) {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.to_string();
        async move {
            runtime
                .git_diff_hunks_continued_with_range_and_page_bytes(
                    project,
                    paths,
                    Some(max_hunks),
                    Some(max_hunk_lines),
                    max_page_bytes,
                    cached,
                    Some(base_commit),
                    Some(head_commit),
                    continuation,
                )
                .await
        }
    });
    let mut scripts = Vec::new();
    let mut page_stdout_bytes = 0usize;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    for _ in 0..16 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "committed read_git_diff_hunks did not finish within 10 seconds for client {client_id}"
        );
        if task.is_finished() {
            break;
        }
        if let Some(request) = probe_patch_agent_request(runtime, client_id).await {
            assert_eq!(request.kind, "run_internal_posix_script");
            assert_eq!(
                request.cwd.as_deref(),
                Some(repo.to_string_lossy().as_ref())
            );
            assert!(request.command.is_empty());
            let payload = request
                .script
                .as_ref()
                .expect("committed read_git_diff_hunks must use typed internal scripts");
            let script = payload.script.clone();
            assert!(script.contains("GIT_NO_REPLACE_OBJECTS=1"));
            assert!(script.contains("GIT_NO_LAZY_FETCH=1"));
            assert!(script.contains("GIT_OPTIONAL_LOCKS=0"));
            assert!(script.contains("GIT_CONFIG_GLOBAL=/dev/null"));
            assert!(script.contains("attributesFile = /dev/null"));
            assert!(script.contains("git read-tree "));
            assert!(script.contains("git ls-files -z -- .gitattributes ':(glob)**/.gitattributes'"));
            assert!(script.contains("git checkout-index -z --stdin --prefix=\"$view/worktree/\""));
            assert!(!script.contains("checkout-index -a"));
            assert!(!script.contains("checkout-index --all"));
            for forbidden in [
                "git fetch",
                "git apply",
                "git commit",
                "git checkout ",
                "git reset",
                "git push",
                "git stash",
                "git rebase",
                "git clean",
                "git add ",
            ] {
                assert!(
                    !script.contains(forbidden),
                    "committed read_git_diff_hunks must remain read-only; found {forbidden}: {script}"
                );
            }
            if script.contains(" diff ") {
                assert!(script.contains("--no-ext-diff"));
                assert!(script.contains("--no-textconv"));
            }
            let (exit_code, stdout, stderr) = run_runner_shell_request_locally(&request);
            if script.contains("page_budget=") {
                page_stdout_bytes = stdout.len();
            }
            complete_patch_agent_request(
                runtime,
                client_id,
                &request.request_id,
                exit_code,
                &stdout,
                &stderr,
            )
            .await;
            scripts.push(script);
        } else {
            tokio::task::yield_now().await;
        }
    }
    assert!(
        task.is_finished(),
        "committed read_git_diff_hunks exceeded its 16-request protocol bound for client {client_id}"
    );
    (task.await.unwrap(), page_stdout_bytes, scripts)
}

#[tokio::test]
async fn git_diff_hunks_committed_exact_range_isolated_targeted_and_head_attributed() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    write_git_review_fixture_file(tmp.path(), "src/a.rs", "pub fn a() -> u8 { 1 }\n");
    write_git_review_fixture_file(tmp.path(), "src/b.rs", "pub fn b() -> u8 { 1 }\n");
    write_git_review_fixture_file(
        tmp.path(),
        "src/space file.rs",
        "pub fn spaced() -> u8 { 1 }\n",
    );
    write_git_review_fixture_file(tmp.path(), "src/你好.rs", "pub fn unicode() -> u8 { 1 }\n");
    write_git_review_fixture_file(tmp.path(), "src/old.rs", "pub fn renamed() -> u8 { 1 }\n");
    write_git_review_fixture_file(tmp.path(), "src/delete.rs", "pub fn deleted() {}\n");
    fs::write(tmp.path().join("asset.bin"), [0u8, 1, 2, 3, 0, 4]).unwrap();
    let base = commit_git_review_fixture(tmp.path(), "base");

    write_git_review_fixture_file(tmp.path(), "src/a.rs", "pub fn a() -> u8 { 2 }\n");
    write_git_review_fixture_file(tmp.path(), "src/b.rs", "pub fn b() -> u8 { 2 }\n");
    write_git_review_fixture_file(
        tmp.path(),
        "src/space file.rs",
        "pub fn spaced() -> u8 { 2 }\n",
    );
    write_git_review_fixture_file(tmp.path(), "src/你好.rs", "pub fn unicode() -> u8 { 2 }\n");
    fs::rename(tmp.path().join("src/old.rs"), tmp.path().join("src/new.rs")).unwrap();
    fs::remove_file(tmp.path().join("src/delete.rs")).unwrap();
    write_git_review_fixture_file(tmp.path(), "src/add.rs", "pub fn added() {}\n");
    fs::write(tmp.path().join("asset.bin"), [0u8, 255, 2, 3, 0, 4]).unwrap();
    write_git_review_fixture_file(tmp.path(), ".gitattributes", "src/b.rs -diff\n");
    let head = commit_git_review_fixture(tmp.path(), "head");

    write_git_review_fixture_file(tmp.path(), "src/a.rs", "DIRTY_WORKTREE_MUST_NOT_APPEAR\n");
    write_git_review_fixture_file(
        tmp.path(),
        ".gitattributes",
        "src/a.rs -diff\nsrc/b.rs diff\n",
    );
    fs::create_dir_all(tmp.path().join(".git/info")).unwrap();
    fs::write(
        tmp.path().join(".git/info/attributes"),
        "src/a.rs -diff\nsrc/b.rs diff\n",
    )
    .unwrap();
    git_test_command_ok(tmp.path(), "git config diff.external false");
    git_test_command_ok(tmp.path(), "git config diff.custom.textconv false");

    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "committed-targeted", "repo", tmp.path())
            .await;
    let paths = vec![
        "src/a.rs".to_string(),
        "src/b.rs".to_string(),
        "src/space file.rs".to_string(),
        "src/你好.rs".to_string(),
    ];
    let (result, raw_bytes, scripts) = run_runner_git_diff_hunks_committed_page(
        &runtime,
        "committed-targeted",
        &project,
        tmp.path(),
        Some(paths.clone()),
        20,
        120,
        base.clone(),
        head.clone(),
        None,
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["scope"]["mode"], "committed");
    assert_eq!(result.output["scope"]["requested_base"], base);
    assert_eq!(result.output["scope"]["requested_head"], head);
    assert_eq!(result.output["scope"]["merge_base"], base);
    assert_eq!(result.output["scope"]["base_is_ancestor"], true);
    assert_eq!(result.output["paths"], json!(paths));
    assert!(
        raw_bytes < 48 * 1024,
        "page producer output must remain bounded"
    );
    assert_eq!(scripts.len(), 2, "scope + page observation expected");
    let serialized = serde_json::to_string(&result.output).unwrap();
    assert!(!serialized.contains("DIRTY_WORKTREE_MUST_NOT_APPEAR"));
    assert!(serialized.contains("space file.rs"));
    assert!(serialized.contains("你好.rs"));
    let files = result.output["files"].as_array().unwrap();
    let a = files
        .iter()
        .find(|file| {
            file["path"]
                .as_str()
                .is_some_and(|path| path.ends_with("a.rs"))
        })
        .expect("src/a.rs committed hunk");
    assert!(!a["hunks"].as_array().unwrap().is_empty());
    let b = files
        .iter()
        .find(|file| {
            file["path"]
                .as_str()
                .is_some_and(|path| path.ends_with("b.rs"))
        })
        .expect("src/b.rs committed file");
    assert_eq!(
        b["binary"], true,
        "reviewed-head attributes must be authoritative"
    );
    assert!(b["hunks"].as_array().unwrap().is_empty());

    let (all_result, _, _) = run_runner_git_diff_hunks_committed_page(
        &runtime,
        "committed-targeted",
        &project,
        tmp.path(),
        None,
        40,
        120,
        base,
        head,
        None,
    )
    .await;
    assert!(all_result.success, "{:?}", all_result.error);
    let all_files = all_result.output["files"].as_array().unwrap();
    assert!(all_files.iter().any(|file| file["status"] == "renamed"));
    assert!(all_files.iter().any(|file| file["status"] == "deleted"));
    assert!(all_files.iter().any(|file| file["status"] == "added"));
    assert!(all_files.iter().any(|file| file["binary"] == true));
}

#[cfg(unix)]
#[tokio::test]
async fn git_diff_hunks_ignores_external_diff_helpers_in_worktree_and_cached_modes() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = crate::test_support::executable_tempdir();
    init_git_repo(tmp.path());
    write_git_review_fixture_file(tmp.path(), "safe.txt", "safe-old\n");
    write_git_review_fixture_file(tmp.path(), ".env", "FAKE_PRIVATE_MARKER_117\n");
    commit_git_review_fixture(tmp.path(), "base");
    write_git_review_fixture_file(tmp.path(), "safe.txt", "safe-new\n");
    let helper = tmp.path().join("extdiff.sh");
    fs::write(
        &helper,
        "#!/bin/sh\nprintf '%s\\n' 'diff --git a/safe.txt b/safe.txt' '--- a/safe.txt' '+++ b/safe.txt' '@@ -1 +1 @@' '-safe-old'\nprintf '+%s\\n' \"$(cat .env)\"\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&helper).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&helper, permissions).unwrap();
    git_test_command_ok(
        tmp.path(),
        &format!(
            "git config diff.external {}",
            shell_escape_simple(helper.to_string_lossy().as_ref())
        ),
    );

    let runtime = test_runtime();
    let project = register_structured_git_agent_at_path(
        &runtime,
        "diff-hunks-no-external",
        "repo",
        tmp.path(),
    )
    .await;
    let paths = Some(vec!["safe.txt".to_string()]);
    let (worktree, _, worktree_script) = run_runner_git_diff_hunks_page(
        &runtime,
        "diff-hunks-no-external",
        &project,
        tmp.path(),
        paths.clone(),
        10,
        80,
        false,
        None,
    )
    .await;
    assert!(worktree.success, "{:?}", worktree.error);
    assert!(worktree_script.contains("--no-ext-diff"));
    assert!(worktree_script.contains("--no-textconv"));
    let worktree_output = serde_json::to_string(&worktree.output).unwrap();
    assert!(worktree_output.contains("safe-new"));
    assert!(!worktree_output.contains("FAKE_PRIVATE_MARKER_117"));

    git_test_command_ok(tmp.path(), "git add -- safe.txt");
    let (cached, _, cached_script) = run_runner_git_diff_hunks_page(
        &runtime,
        "diff-hunks-no-external",
        &project,
        tmp.path(),
        paths,
        10,
        80,
        true,
        None,
    )
    .await;
    assert!(cached.success, "{:?}", cached.error);
    assert!(cached_script.contains("--no-ext-diff"));
    assert!(cached_script.contains("--no-textconv"));
    let cached_output = serde_json::to_string(&cached.output).unwrap();
    assert!(cached_output.contains("safe-new"));
    assert!(!cached_output.contains("FAKE_PRIVATE_MARKER_117"));
}

#[tokio::test]
async fn git_diff_hunks_never_returns_secret_path_content_in_any_mode() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    write_git_review_fixture_file(tmp.path(), ".env", "API_TOKEN=base-secret\n");
    write_git_review_fixture_file(tmp.path(), "secret key.pem", "API_TOKEN=worktree-base\n");
    let base = commit_git_review_fixture(tmp.path(), "secret base");
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::rename(tmp.path().join(".env"), tmp.path().join("src/config.rs")).unwrap();
    write_git_review_fixture_file(tmp.path(), "src/config.rs", "API_TOKEN=committed-secret\n");
    let head = commit_git_review_fixture(tmp.path(), "secret head");
    write_git_review_fixture_file(tmp.path(), "secret key.pem", "API_TOKEN=dirty-secret\n");

    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "diff-secret-boundary", "repo", tmp.path())
            .await;

    let (committed, _, _) = run_runner_git_diff_hunks_committed_page(
        &runtime,
        "diff-secret-boundary",
        &project,
        tmp.path(),
        None,
        20,
        120,
        base,
        head,
        None,
    )
    .await;
    assert!(!committed.success);
    assert_eq!(committed.output["reason_code"], "sensitive_path");
    let committed_serialized = serde_json::to_string(&committed).unwrap();
    for secret in [
        "base-secret",
        "committed-secret",
        "worktree-base",
        "dirty-secret",
    ] {
        assert!(
            !committed_serialized.contains(secret),
            "committed diff leaked protected content: {committed_serialized}"
        );
    }

    let (worktree, _, _) = run_runner_git_diff_hunks_page(
        &runtime,
        "diff-secret-boundary",
        &project,
        tmp.path(),
        None,
        20,
        120,
        false,
        None,
    )
    .await;
    assert!(
        !worktree.success,
        "unexpected protected diff success: {worktree:?}"
    );
    assert_eq!(worktree.output["reason_code"], "sensitive_path");
    let worktree_serialized = serde_json::to_string(&worktree).unwrap();
    assert!(!worktree_serialized.contains("dirty-secret"));

    let explicit = runtime
        .git_diff_hunks_continued(
            project,
            Some(vec![".env".to_string()]),
            Some(20),
            Some(120),
            Some(false),
            None,
        )
        .await;
    assert!(!explicit.success);
    assert_eq!(explicit.output["reason_code"], "sensitive_path");
    assert!(
        probe_patch_agent_request(&runtime, "diff-secret-boundary")
            .await
            .is_none(),
        "explicit protected path must fail before Runner dispatch"
    );
}

#[tokio::test]
async fn git_diff_hunks_committed_range_validation_and_merge_base_fail_closed() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    write_git_review_fixture_file(tmp.path(), "base.txt", "base\n");
    let base = commit_git_review_fixture(tmp.path(), "base");
    write_git_review_fixture_file(tmp.path(), "base.txt", "head\n");
    let head = commit_git_review_fixture(tmp.path(), "head");
    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "committed-validation", "repo", tmp.path())
            .await;

    for (base_arg, head_arg, cached, reason) in [
        (
            Some("HEAD".to_string()),
            Some(head.clone()),
            None,
            "invalid_commit_id",
        ),
        (
            Some(base.clone()),
            None,
            None,
            "committed_range_requires_base_and_head",
        ),
        (
            Some(base.clone()),
            Some(head.clone()),
            Some(true),
            "committed_range_conflicts_with_cached",
        ),
    ] {
        let result = runtime
            .git_diff_hunks_continued_with_range(
                project.clone(),
                None,
                Some(10),
                Some(80),
                cached,
                base_arg,
                head_arg,
                None,
            )
            .await;
        assert!(!result.success);
        assert_eq!(result.output["reason_code"], reason);
        assert!(
            probe_patch_agent_request(&runtime, "committed-validation")
                .await
                .is_none(),
            "invalid committed input must fail before Runner dispatch"
        );
    }

    let (same, _, _) = run_runner_git_diff_hunks_committed_page(
        &runtime,
        "committed-validation",
        &project,
        tmp.path(),
        None,
        10,
        80,
        base.clone(),
        base.clone(),
        None,
    )
    .await;
    assert!(same.success, "{:?}", same.error);
    assert_eq!(same.output["hunk_count"], 0);
    assert_eq!(same.output["files"], json!([]));

    let missing = "f".repeat(40);
    let (missing_result, _, missing_scripts) = run_runner_git_diff_hunks_committed_page(
        &runtime,
        "committed-validation",
        &project,
        tmp.path(),
        None,
        10,
        80,
        base.clone(),
        missing.clone(),
        None,
    )
    .await;
    assert!(!missing_result.success);
    assert_eq!(
        missing_result.output["reason_code"],
        "head_commit_missing_or_not_commit"
    );
    assert_eq!(
        missing_scripts.len(),
        1,
        "missing object must stop before page diff"
    );

    let (blob_exit, blob_stdout, blob_stderr, _) =
        run_command_sync("printf blob | git hash-object -w --stdin", tmp.path(), 30);
    assert_eq!(blob_exit, 0, "{blob_stderr}");
    let blob = blob_stdout.trim().to_string();
    let (blob_result, _, blob_scripts) = run_runner_git_diff_hunks_committed_page(
        &runtime,
        "committed-validation",
        &project,
        tmp.path(),
        None,
        10,
        80,
        base,
        blob,
        None,
    )
    .await;
    assert!(!blob_result.success);
    assert_eq!(
        blob_result.output["reason_code"],
        "head_commit_missing_or_not_commit"
    );
    assert_eq!(blob_scripts.len(), 1);
}

#[tokio::test]
async fn git_diff_hunks_committed_nonancestor_disconnected_and_ambiguous_merge_base() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    write_git_review_fixture_file(tmp.path(), "root.txt", "root\n");
    let root = commit_git_review_fixture(tmp.path(), "root");
    let (_, root_branch, _, _) = run_command_sync("git branch --show-current", tmp.path(), 30);
    let root_branch = root_branch.trim().to_string();
    git_test_command_ok(tmp.path(), "git checkout -b feature");
    write_git_review_fixture_file(tmp.path(), "feature.txt", "feature\n");
    let feature = commit_git_review_fixture(tmp.path(), "feature");
    git_test_command_ok(
        tmp.path(),
        &format!("git checkout {}", shell_escape_simple(&root_branch)),
    );
    write_git_review_fixture_file(tmp.path(), "main.txt", "main\n");
    let requested_base = commit_git_review_fixture(tmp.path(), "main");

    let runtime = test_runtime();
    let project =
        register_structured_git_agent_at_path(&runtime, "committed-merge-base", "repo", tmp.path())
            .await;
    let (nonancestor, _, _) = run_runner_git_diff_hunks_committed_page(
        &runtime,
        "committed-merge-base",
        &project,
        tmp.path(),
        None,
        10,
        80,
        requested_base.clone(),
        feature.clone(),
        None,
    )
    .await;
    assert!(nonancestor.success, "{:?}", nonancestor.error);
    assert_eq!(
        nonancestor.output["scope"]["requested_base"],
        requested_base
    );
    assert_eq!(nonancestor.output["scope"]["requested_head"], feature);
    assert_eq!(nonancestor.output["scope"]["merge_base"], root);
    assert_eq!(nonancestor.output["scope"]["base_is_ancestor"], false);

    let disconnected = tempfile::tempdir().unwrap();
    init_git_repo(disconnected.path());
    write_git_review_fixture_file(disconnected.path(), "one.txt", "one\n");
    let first = commit_git_review_fixture(disconnected.path(), "one");
    git_test_command_ok(disconnected.path(), "git checkout --orphan other");
    git_test_command_ok(disconnected.path(), "git rm -rf .");
    write_git_review_fixture_file(disconnected.path(), "two.txt", "two\n");
    let second = commit_git_review_fixture(disconnected.path(), "two");
    let runtime2 = test_runtime();
    let project2 = register_structured_git_agent_at_path(
        &runtime2,
        "committed-disconnected",
        "repo",
        disconnected.path(),
    )
    .await;
    let (no_base, _, scripts) = run_runner_git_diff_hunks_committed_page(
        &runtime2,
        "committed-disconnected",
        &project2,
        disconnected.path(),
        None,
        10,
        80,
        first,
        second,
        None,
    )
    .await;
    assert!(!no_base.success);
    assert_eq!(no_base.output["reason_code"], "no_merge_base");
    assert_eq!(scripts.len(), 1);

    let ambiguous = tempfile::tempdir().unwrap();
    init_git_repo(ambiguous.path());
    write_git_review_fixture_file(ambiguous.path(), "root.txt", "root\n");
    commit_git_review_fixture(ambiguous.path(), "root");
    git_test_command_ok(ambiguous.path(), "git checkout -b side-a");
    write_git_review_fixture_file(ambiguous.path(), "a.txt", "a\n");
    let a1 = commit_git_review_fixture(ambiguous.path(), "a1");
    git_test_command_ok(ambiguous.path(), "git checkout -b side-b HEAD~1");
    write_git_review_fixture_file(ambiguous.path(), "b.txt", "b\n");
    let b1 = commit_git_review_fixture(ambiguous.path(), "b1");
    git_test_command_ok(ambiguous.path(), "git checkout side-a");
    git_test_command_ok(
        ambiguous.path(),
        &format!("git merge --no-ff -m merge-b {}", shell_escape_simple(&b1)),
    );
    let (_, a2, _, _) = run_command_sync("git rev-parse HEAD", ambiguous.path(), 30);
    let a2 = a2.trim().to_string();
    git_test_command_ok(ambiguous.path(), "git checkout side-b");
    git_test_command_ok(
        ambiguous.path(),
        &format!("git merge --no-ff -m merge-a {}", shell_escape_simple(&a1)),
    );
    let (_, b2, _, _) = run_command_sync("git rev-parse HEAD", ambiguous.path(), 30);
    let b2 = b2.trim().to_string();
    let runtime3 = test_runtime();
    let project3 = register_structured_git_agent_at_path(
        &runtime3,
        "committed-ambiguous",
        "repo",
        ambiguous.path(),
    )
    .await;
    let (ambiguous_result, _, ambiguous_scripts) = run_runner_git_diff_hunks_committed_page(
        &runtime3,
        "committed-ambiguous",
        &project3,
        ambiguous.path(),
        None,
        10,
        80,
        a2,
        b2,
        None,
    )
    .await;
    assert!(!ambiguous_result.success);
    assert_eq!(
        ambiguous_result.output["reason_code"],
        "ambiguous_merge_base"
    );
    assert_eq!(ambiguous_scripts.len(), 1);
}

fn git_diff_hunks_byte_fragment_bodies() -> (String, String) {
    let base = (0..1000)
        .map(|line| format!("line-{line:04}\n"))
        .collect::<String>();
    let changed = (0..1000)
        .map(|line| {
            if line == 20 {
                "small-a-changed\n".to_string()
            } else if (350..440).contains(&line) {
                format!("large-b-{line:04}-{}\n", "y".repeat(360))
            } else if line == 800 {
                "small-c-changed\n".to_string()
            } else {
                format!("line-{line:04}\n")
            }
        })
        .collect::<String>();
    (base, changed)
}

fn authoritative_text_hunks(raw_diff: &str) -> Vec<String> {
    let starts = raw_diff
        .match_indices("@@ ")
        .filter_map(|(index, _)| {
            (index == 0 || raw_diff.as_bytes().get(index.wrapping_sub(1)) == Some(&b'\n'))
                .then_some(index)
        })
        .collect::<Vec<_>>();
    starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let end = starts.get(index + 1).copied().unwrap_or(raw_diff.len());
            raw_diff[*start..end].trim_end_matches('\n').to_string()
        })
        .collect()
}

fn git_diff_hunks_fragment_cursor(token: &str) -> (u8, usize, usize) {
    use base64::{engine::general_purpose, Engine as _};

    let encoded = token.strip_prefix("wcdh2.").expect("wcdh2 token");
    let payload = general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .expect("base64url token payload");
    let tag = payload[0];
    assert!(matches!(tag, 3 | 4), "expected hunk-fragment token");
    let fence_len = payload[33] as usize;
    let cursor = 34 + fence_len;
    let record_index = u64::from_be_bytes(payload[cursor..cursor + 8].try_into().unwrap());
    let next_line = u64::from_be_bytes(payload[cursor + 8..cursor + 16].try_into().unwrap());
    (
        tag,
        usize::try_from(record_index).unwrap(),
        usize::try_from(next_line).unwrap(),
    )
}

#[tokio::test]
async fn git_diff_hunks_byte_budget_later_record_fragments_by_complete_line() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    let (base_body, changed_body) = git_diff_hunks_byte_fragment_bodies();
    write_git_review_fixture_file(repo.path(), "byte-fragments.txt", &base_body);
    commit_git_review_fixture(repo.path(), "byte fragment base");
    write_git_review_fixture_file(repo.path(), "byte-fragments.txt", &changed_body);

    let (raw_exit, raw_diff, raw_stderr) = run_command_full_capture(
        "git diff --unified=80 -- byte-fragments.txt",
        repo.path(),
        30,
    );
    assert_eq!(raw_exit, 0, "raw worktree diff failed: {raw_stderr}");
    let authoritative = authoritative_text_hunks(&raw_diff);
    assert_eq!(authoritative.len(), 3, "fixture must contain A, B, C hunks");
    assert!(authoritative[1].len() > MIN_GIT_DIFF_HUNKS_PAGE_BYTES);
    assert!(authoritative[1].lines().count() < 400);

    let runtime = test_runtime();
    let client_id = "byte-budget-worktree-fragment";
    let project =
        register_structured_git_agent_at_path(&runtime, client_id, "repo", repo.path()).await;
    let paths = Some(vec!["byte-fragments.txt".to_string()]);

    let (first, _, _) = run_runner_git_diff_hunks_page_with_budget(
        &runtime,
        client_id,
        &project,
        repo.path(),
        paths.clone(),
        10,
        400,
        Some(MIN_GIT_DIFF_HUNKS_PAGE_BYTES),
        false,
        None,
    )
    .await;
    assert!(first.success, "{first:?}");
    assert_eq!(first.output["hunk_count"], 1);
    assert_eq!(
        first.output["files"][0]["hunks"][0]["diff"],
        authoritative[0]
    );
    assert_eq!(
        first.output["truncation_reasons"],
        json!(["page_byte_budget"])
    );
    let later_call = first.output["recovery"]["later_hunks"]["next_call"].clone();
    assert_eq!(
        later_call["arguments"]["max_page_bytes"],
        MIN_GIT_DIFF_HUNKS_PAGE_BYTES
    );

    let large = run_parser_ready_worktree_git_diff_hunks_call(
        &runtime,
        client_id,
        repo.path(),
        &later_call,
    )
    .await;
    assert!(large.success, "{large:?}");
    assert_eq!(large.output["hunk_count"], 1);
    assert_eq!(
        large.output["truncation_reasons"],
        json!(["page_byte_budget"])
    );
    let large_hunk = &large.output["files"][0]["hunks"][0];
    assert_eq!(
        large_hunk["header"],
        authoritative[1].lines().next().unwrap()
    );
    assert_eq!(large_hunk["truncated"], true);
    assert_eq!(
        large.output["recovery"]["current_hunk"]["reason_code"],
        "hunk_fragment_continuation_available"
    );
    assert!(large.output["recovery"]["later_hunks"]["next_call"].is_object());
    assert_ne!(
        large.output["recovery"]["current_hunk"]["next_call"]["arguments"]["continuation"],
        large.output["recovery"]["later_hunks"]["next_call"]["arguments"]["continuation"]
    );

    let mut reconstructed = large_hunk["diff"].as_str().unwrap().to_string();
    let mut current_call = large.output["recovery"]["current_hunk"]["next_call"].clone();
    let first_token = current_call["arguments"]["continuation"].as_str().unwrap();
    let (tag, record_index, mut next_line) = git_diff_hunks_fragment_cursor(first_token);
    assert_eq!(tag, 3, "worktree fragment token tag");
    assert!(next_line > 1);
    let mut seen_tokens = HashSet::from([first_token.to_string()]);
    let mut fragment_calls = 0usize;
    let final_later_call;
    loop {
        let page = run_parser_ready_worktree_git_diff_hunks_call(
            &runtime,
            client_id,
            repo.path(),
            &current_call,
        )
        .await;
        assert!(page.success, "{page:?}");
        assert_eq!(page.output["hunk_count"], 1);
        let hunk = &page.output["files"][0]["hunks"][0];
        assert_eq!(hunk["header"], large_hunk["header"]);
        assert_eq!(hunk["continued"], true);
        let body = hunk["diff"].as_str().unwrap();
        assert!(
            !body.is_empty(),
            "every fragment must make complete-line progress"
        );
        reconstructed.push('\n');
        reconstructed.push_str(body);
        fragment_calls += 1;

        let Some(next_call) = page
            .output
            .get("recovery")
            .and_then(|recovery| recovery.get("current_hunk"))
            .and_then(|current| current.get("next_call"))
        else {
            final_later_call = page.output["recovery"]["later_hunks"]["next_call"].clone();
            break;
        };
        let token = next_call["arguments"]["continuation"].as_str().unwrap();
        assert!(
            seen_tokens.insert(token.to_string()),
            "fragment token repeated"
        );
        let (next_tag, next_record, advanced_line) = git_diff_hunks_fragment_cursor(token);
        assert_eq!(next_tag, 3);
        assert_eq!(next_record, record_index, "fragment changed logical hunk");
        assert!(advanced_line > next_line, "next_line must strictly advance");
        next_line = advanced_line;
        current_call = next_call.clone();
    }
    assert!(
        fragment_calls >= 2,
        "fixture must require repeated byte-bounded fragments"
    );
    assert_eq!(reconstructed, authoritative[1]);

    let tail = run_parser_ready_worktree_git_diff_hunks_call(
        &runtime,
        client_id,
        repo.path(),
        &final_later_call,
    )
    .await;
    assert!(tail.success, "{tail:?}");
    assert_eq!(tail.output["hunk_count"], 1);
    assert_eq!(
        tail.output["files"][0]["hunks"][0]["diff"],
        authoritative[2]
    );
    assert_eq!(tail.output["has_more"], false);
    assert!(tail.output.get("recovery").is_none());

    let (line_bound, _, _) = run_runner_git_diff_hunks_page_with_budget(
        &runtime,
        client_id,
        &project,
        repo.path(),
        paths,
        10,
        160,
        Some(MIN_GIT_DIFF_HUNKS_PAGE_BYTES),
        false,
        None,
    )
    .await;
    assert!(line_bound.success, "{line_bound:?}");
    assert!(line_bound.output["truncation_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason == "hunk_line_limit"));
    let line_fragment_call = &line_bound.output["recovery"]["current_hunk"]["next_call"];
    let line_fragment = run_parser_ready_worktree_git_diff_hunks_call(
        &runtime,
        client_id,
        repo.path(),
        line_fragment_call,
    )
    .await;
    assert!(line_fragment.success, "{line_fragment:?}");
    assert_eq!(
        line_fragment.output["files"][0]["hunks"][0]["continued"],
        true
    );
}

#[tokio::test]
async fn git_diff_hunks_committed_byte_budget_fragment_token_continues_exact_hunk() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    let (base_body, changed_body) = git_diff_hunks_byte_fragment_bodies();
    write_git_review_fixture_file(repo.path(), "byte-fragments.txt", &base_body);
    let base = commit_git_review_fixture(repo.path(), "committed byte fragment base");
    write_git_review_fixture_file(repo.path(), "byte-fragments.txt", &changed_body);
    let head = commit_git_review_fixture(repo.path(), "committed byte fragment head");

    let runtime = test_runtime();
    let client_id = "byte-budget-committed-fragment";
    let project =
        register_structured_git_agent_at_path(&runtime, client_id, "repo", repo.path()).await;
    let paths = Some(vec!["byte-fragments.txt".to_string()]);
    let (first, _, _) = run_runner_git_diff_hunks_committed_page_with_budget(
        &runtime,
        client_id,
        &project,
        repo.path(),
        paths,
        10,
        400,
        Some(MIN_GIT_DIFF_HUNKS_PAGE_BYTES),
        base.clone(),
        head.clone(),
        None,
    )
    .await;
    assert!(first.success, "{first:?}");
    assert_eq!(first.output["hunk_count"], 1);
    assert_eq!(
        first.output["truncation_reasons"],
        json!(["page_byte_budget"])
    );
    let later_call = first.output["recovery"]["later_hunks"]["next_call"].clone();
    assert_eq!(later_call["arguments"]["base_commit"], base);
    assert_eq!(later_call["arguments"]["head_commit"], head);
    assert_eq!(
        later_call["arguments"]["max_page_bytes"],
        MIN_GIT_DIFF_HUNKS_PAGE_BYTES
    );

    let large = run_parser_ready_committed_git_diff_hunks_call(
        &runtime,
        client_id,
        repo.path(),
        &later_call,
    )
    .await;
    assert!(large.success, "{large:?}");
    assert_eq!(large.output["hunk_count"], 1);
    assert!(large.output["truncation_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason == "page_byte_budget"));
    assert_eq!(
        large.output["recovery"]["current_hunk"]["reason_code"],
        "hunk_fragment_continuation_available"
    );
    let current_call = large.output["recovery"]["current_hunk"]["next_call"].clone();
    let current_token = current_call["arguments"]["continuation"].as_str().unwrap();
    let (tag, record_index, next_line) = git_diff_hunks_fragment_cursor(current_token);
    assert_eq!(tag, 4, "committed fragment token tag");
    assert!(next_line > 1);

    let continued = run_parser_ready_committed_git_diff_hunks_call(
        &runtime,
        client_id,
        repo.path(),
        &current_call,
    )
    .await;
    assert!(continued.success, "{continued:?}");
    assert_eq!(continued.output["hunk_count"], 1);
    assert_eq!(continued.output["files"][0]["hunks"][0]["continued"], true);
    let next_call = &continued.output["recovery"]["current_hunk"]["next_call"];
    let next_token = next_call["arguments"]["continuation"].as_str().unwrap();
    let (next_tag, next_record, advanced_line) = git_diff_hunks_fragment_cursor(next_token);
    assert_eq!(next_tag, 4);
    assert_eq!(next_record, record_index);
    assert!(advanced_line > next_line);
}

#[tokio::test]
async fn git_diff_hunks_same_file_multi_hunk_pages_preserve_projection_and_continuation() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let base_body = (0..1400)
        .map(|line| format!("line-{line:04}\n"))
        .collect::<String>();
    write_git_review_fixture_file(tmp.path(), "multi.txt", &base_body);
    let base = commit_git_review_fixture(tmp.path(), "base");
    let head_body = (0..1400)
        .map(|line| {
            if line == 10 || (300..850).contains(&line) || line == 1200 {
                format!("changed-{line:04}\n")
            } else {
                format!("line-{line:04}\n")
            }
        })
        .collect::<String>();
    write_git_review_fixture_file(tmp.path(), "multi.txt", &head_body);
    let head = commit_git_review_fixture(tmp.path(), "head");

    let runtime = test_runtime();
    let client_id = "same-file-multi-hunk";
    let project =
        register_structured_git_agent_at_path(&runtime, client_id, "repo", tmp.path()).await;
    let paths = Some(vec!["multi.txt".to_string()]);

    let (one, _, _) = run_runner_git_diff_hunks_committed_page_with_budget(
        &runtime,
        client_id,
        &project,
        tmp.path(),
        paths.clone(),
        1,
        400,
        Some(196_608),
        base.clone(),
        head.clone(),
        None,
    )
    .await;
    assert!(one.success, "{one:?}");
    assert_eq!(one.output["hunk_count"], 1);
    assert_eq!(one.output["has_more"], true);
    let one_next = &one.output["recovery"]["later_hunks"]["next_call"];
    crate::tool_runtime::ToolCall::from_tool_name(
        one_next["tool"].as_str().unwrap(),
        one_next["arguments"].clone(),
    )
    .expect("max_hunks=1 later-record continuation must remain parser-ready");

    let (two, _, _) = run_runner_git_diff_hunks_committed_page_with_budget(
        &runtime,
        client_id,
        &project,
        tmp.path(),
        paths.clone(),
        2,
        400,
        Some(196_608),
        base.clone(),
        head.clone(),
        None,
    )
    .await;
    assert!(two.success, "{two:?}");
    assert_eq!(two.output["hunk_count"], 2);
    assert_eq!(two.output["files"].as_array().unwrap().len(), 1);
    let first_page_hunks = two.output["files"][0]["hunks"].as_array().unwrap();
    assert_eq!(first_page_hunks.len(), 2);
    assert_eq!(two.output["has_more"], true);
    assert_eq!(
        two.output["recovery"]["current_hunk"]["reason_code"],
        "hunk_fragment_continuation_available"
    );
    let later = &two.output["recovery"]["later_hunks"]["next_call"];
    crate::tool_runtime::ToolCall::from_tool_name(
        later["tool"].as_str().unwrap(),
        later["arguments"].clone(),
    )
    .expect("same-file multi-hunk later-record continuation must remain parser-ready");
    let continuation = later["arguments"]["continuation"]
        .as_str()
        .unwrap()
        .to_string();
    let mut headers = first_page_hunks
        .iter()
        .map(|hunk| hunk["header"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();

    let (tail, _, _) = run_runner_git_diff_hunks_committed_page_with_budget(
        &runtime,
        client_id,
        &project,
        tmp.path(),
        paths.clone(),
        2,
        400,
        Some(196_608),
        base.clone(),
        head.clone(),
        Some(continuation),
    )
    .await;
    assert!(tail.success, "{tail:?}");
    assert_eq!(tail.output["hunk_count"], 1);
    assert_eq!(tail.output["has_more"], false);
    assert!(tail.output.get("recovery").is_none());
    headers.push(
        tail.output["files"][0]["hunks"][0]["header"]
            .as_str()
            .unwrap()
            .to_string(),
    );
    let unique = headers.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(headers.len(), 3);
    assert_eq!(
        unique.len(),
        3,
        "pagination must not duplicate or skip hunk records"
    );

    let dirty_body = (0..1400)
        .map(|line| {
            if line == 20 || (320..870).contains(&line) || line == 1250 {
                format!("dirty-{line:04}\n")
            } else if line == 10 || (300..850).contains(&line) || line == 1200 {
                format!("changed-{line:04}\n")
            } else {
                format!("line-{line:04}\n")
            }
        })
        .collect::<String>();
    write_git_review_fixture_file(tmp.path(), "multi.txt", &dirty_body);
    let (worktree, _, _) = run_runner_git_diff_hunks_page_with_budget(
        &runtime,
        client_id,
        &project,
        tmp.path(),
        paths,
        2,
        400,
        Some(196_608),
        false,
        None,
    )
    .await;
    assert!(worktree.success, "{worktree:?}");
    assert_eq!(worktree.output["hunk_count"], 2);
    assert_eq!(worktree.output["files"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn git_diff_hunks_committed_continuation_binds_range_paths_mode_and_state() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let base_body = (0..1200)
        .map(|line| format!("line-{line:04}\n"))
        .collect::<String>();
    write_git_review_fixture_file(tmp.path(), "large.txt", &base_body);
    write_git_review_fixture_file(tmp.path(), "other.txt", "same\n");
    let base = commit_git_review_fixture(tmp.path(), "base");
    let head_body = (0..1200)
        .map(|line| {
            if matches!(line, 10 | 310 | 610 | 910) {
                format!("changed-{line:04}\n")
            } else {
                format!("line-{line:04}\n")
            }
        })
        .collect::<String>();
    write_git_review_fixture_file(tmp.path(), "large.txt", &head_body);
    let head = commit_git_review_fixture(tmp.path(), "head");

    let runtime = test_runtime();
    let project = register_structured_git_agent_at_path(
        &runtime,
        "committed-continuation",
        "repo",
        tmp.path(),
    )
    .await;
    let paths = Some(vec!["large.txt".to_string()]);
    // Explicit cached=false is the same committed-range mode as omission. Start
    // with the explicit spelling, then replay the returned continuation through
    // the ordinary omitted-cached helper below to prove canonical identity.
    let (first, first_bytes, first_scripts) =
        run_runner_git_diff_hunks_committed_page_with_options(
            &runtime,
            "committed-continuation",
            &project,
            tmp.path(),
            paths.clone(),
            1,
            120,
            None,
            Some(false),
            base.clone(),
            head.clone(),
            None,
        )
        .await;
    assert!(first.success, "{:?}", first.error);
    assert_eq!(first.output["has_more"], true);
    assert!(first_bytes < 48 * 1024);
    assert_eq!(first_scripts.len(), 2);
    let token = first.output["recovery"]["later_hunks"]["next_call"]["arguments"]["continuation"]
        .as_str()
        .expect("first committed page continuation")
        .to_string();
    assert!(token.starts_with("wcdh2."));
    let recovery = &first.output["recovery"];
    assert!(recovery["later_hunks"].is_object());
    assert_eq!(
        recovery["later_hunks"]["next_call"]["arguments"]["project"],
        project
    );
    assert_eq!(
        recovery["later_hunks"]["next_call"]["arguments"]["paths"],
        json!(["large.txt"])
    );
    assert_eq!(
        recovery["later_hunks"]["next_call"]["arguments"]["base_commit"],
        base
    );
    assert_eq!(
        recovery["later_hunks"]["next_call"]["arguments"]["head_commit"],
        head
    );
    assert_eq!(
        recovery["later_hunks"]["next_call"]["arguments"]["max_hunks"],
        1
    );
    assert_eq!(
        recovery["later_hunks"]["next_call"]["arguments"]["max_hunk_lines"],
        120
    );
    assert_eq!(
        recovery["later_hunks"]["next_call"]["arguments"]["continuation"],
        token
    );
    assert!(recovery["later_hunks"]["next_call"]["arguments"]
        .get("cached")
        .is_none());
    assert_git_diff_hunks_sparse_recovery_calls_parse(recovery);
    assert_eq!(
        recovery["later_hunks"]["next_call"]["arguments"]["max_page_bytes"],
        DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES
    );
    let (page_budget_mismatch, _, mismatch_scripts) =
        run_runner_git_diff_hunks_committed_page_with_budget(
            &runtime,
            "committed-continuation",
            &project,
            tmp.path(),
            paths.clone(),
            1,
            120,
            Some(MIN_GIT_DIFF_HUNKS_PAGE_BYTES),
            base.clone(),
            head.clone(),
            Some(token.clone()),
        )
        .await;
    assert!(!page_budget_mismatch.success);
    assert_eq!(
        page_budget_mismatch.output["reason_code"],
        "continuation_mismatch"
    );
    assert_eq!(
        mismatch_scripts.len(),
        1,
        "committed page-budget mismatch may resolve range scope but must stop before the page producer"
    );

    let first_diff = first.output["files"][0]["hunks"][0]["diff"]
        .as_str()
        .unwrap()
        .to_string();

    let (second, _, _) = run_runner_git_diff_hunks_committed_page(
        &runtime,
        "committed-continuation",
        &project,
        tmp.path(),
        paths.clone(),
        1,
        120,
        base.clone(),
        head.clone(),
        Some(token.clone()),
    )
    .await;
    assert!(second.success, "{:?}", second.error);
    let second_diff = second.output["files"][0]["hunks"][0]["diff"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(
        first_diff, second_diff,
        "continuation must not replay page one"
    );

    let (second_again, _, _) = run_runner_git_diff_hunks_committed_page(
        &runtime,
        "committed-continuation",
        &project,
        tmp.path(),
        paths.clone(),
        1,
        120,
        base.clone(),
        head.clone(),
        Some(token.clone()),
    )
    .await;
    assert!(second_again.success);
    assert_eq!(second_again.output["files"], second.output["files"]);
    assert_eq!(
        second_again.output["recovery"]["later_hunks"]["next_call"]["arguments"]["continuation"],
        second.output["recovery"]["later_hunks"]["next_call"]["arguments"]["continuation"]
    );

    let mut seen = vec![first_diff, second_diff];
    let mut next = second.output["recovery"]["later_hunks"]["next_call"]["arguments"]
        ["continuation"]
        .as_str()
        .map(str::to_string);
    while let Some(current) = next {
        let (page, _, _) = run_runner_git_diff_hunks_committed_page(
            &runtime,
            "committed-continuation",
            &project,
            tmp.path(),
            paths.clone(),
            1,
            120,
            base.clone(),
            head.clone(),
            Some(current),
        )
        .await;
        assert!(page.success, "{:?}", page.error);
        if page.output["hunk_count"].as_u64().unwrap_or(0) > 0 {
            let diff = page.output["files"][0]["hunks"][0]["diff"]
                .as_str()
                .unwrap()
                .to_string();
            assert!(!seen.contains(&diff), "hunk page replayed: {diff}");
            seen.push(diff);
        }
        next = page.output["recovery"]["later_hunks"]["next_call"]["arguments"]["continuation"]
            .as_str()
            .map(str::to_string);
    }
    assert_eq!(
        seen.len(),
        4,
        "all four committed hunks must be returned once"
    );

    let (path_mismatch, _, path_scripts) = run_runner_git_diff_hunks_committed_page(
        &runtime,
        "committed-continuation",
        &project,
        tmp.path(),
        Some(vec!["other.txt".to_string()]),
        1,
        120,
        base.clone(),
        head.clone(),
        Some(token.clone()),
    )
    .await;
    assert!(!path_mismatch.success);
    assert_eq!(path_mismatch.output["reason_code"], "continuation_mismatch");
    assert_eq!(
        path_scripts.len(),
        1,
        "mismatch must stop before page producer"
    );

    let (range_mismatch, _, range_scripts) = run_runner_git_diff_hunks_committed_page(
        &runtime,
        "committed-continuation",
        &project,
        tmp.path(),
        paths.clone(),
        1,
        120,
        base.clone(),
        base.clone(),
        Some(token.clone()),
    )
    .await;
    assert!(!range_mismatch.success);
    assert_eq!(
        range_mismatch.output["reason_code"],
        "continuation_mismatch"
    );
    assert_eq!(range_scripts.len(), 1);

    let mut tampered = token.clone().into_bytes();
    let last = tampered.len() - 1;
    tampered[last] = if tampered[last] == b'A' { b'B' } else { b'A' };
    let tampered = String::from_utf8(tampered).unwrap();
    let (tampered_result, _, tampered_scripts) = run_runner_git_diff_hunks_committed_page(
        &runtime,
        "committed-continuation",
        &project,
        tmp.path(),
        paths.clone(),
        1,
        120,
        base.clone(),
        head.clone(),
        Some(tampered),
    )
    .await;
    assert!(!tampered_result.success);
    assert_eq!(
        tampered_result.output["reason_code"],
        "invalid_continuation"
    );
    assert_eq!(tampered_scripts.len(), 1);

    {
        use base64::{engine::general_purpose, Engine as _};
        let encoded = token.strip_prefix("wcdh2.").unwrap();
        let mut decoded = general_purpose::URL_SAFE_NO_PAD.decode(encoded).unwrap();
        let fence_len = decoded[33] as usize;
        let cursor_last = 34 + fence_len + 7;
        decoded[cursor_last] ^= 1;
        let forged_next = format!("wcdh2.{}", general_purpose::URL_SAFE_NO_PAD.encode(decoded));
        let (forged_result, _, forged_scripts) = run_runner_git_diff_hunks_committed_page(
            &runtime,
            "committed-continuation",
            &project,
            tmp.path(),
            paths.clone(),
            1,
            120,
            base.clone(),
            head.clone(),
            Some(forged_next),
        )
        .await;
        assert!(!forged_result.success);
        assert_eq!(forged_result.output["reason_code"], "invalid_continuation");
        assert_eq!(
            forged_scripts.len(),
            1,
            "pagination-state tamper must fail after scope resolution and before page observation"
        );
    }

    let dirty_body = (0..1200)
        .map(|line| {
            if matches!(line, 20 | 500 | 1000) {
                format!("dirty-{line:04}\n")
            } else {
                if line == 10 || line == 310 || line == 610 || line == 910 {
                    format!("changed-{line:04}\n")
                } else {
                    format!("line-{line:04}\n")
                }
            }
        })
        .collect::<String>();
    write_git_review_fixture_file(tmp.path(), "large.txt", &dirty_body);
    let (worktree_page, _, _) = run_runner_git_diff_hunks_page(
        &runtime,
        "committed-continuation",
        &project,
        tmp.path(),
        paths.clone(),
        1,
        120,
        false,
        None,
    )
    .await;
    assert!(worktree_page.success);
    let worktree_token = worktree_page.output["recovery"]["later_hunks"]["next_call"]["arguments"]
        ["continuation"]
        .as_str()
        .expect("worktree continuation")
        .to_string();
    let (wrong_mode, _, wrong_mode_scripts) = run_runner_git_diff_hunks_committed_page(
        &runtime,
        "committed-continuation",
        &project,
        tmp.path(),
        paths.clone(),
        1,
        120,
        base.clone(),
        head.clone(),
        Some(worktree_token),
    )
    .await;
    assert!(!wrong_mode.success);
    assert_eq!(wrong_mode.output["reason_code"], "continuation_mismatch");
    assert_eq!(wrong_mode_scripts.len(), 1);

    let reverse_mode = runtime
        .git_diff_hunks_continued(project, paths, Some(1), Some(120), Some(false), Some(token))
        .await;
    assert!(!reverse_mode.success);
    assert_eq!(reverse_mode.output["reason_code"], "continuation_mismatch");
}

#[tokio::test]
async fn git_diff_hunks_committed_hunk_fragment_continuation_is_mac_bound_and_mode_isolated() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    let original = (0..600)
        .map(|line| format!("old-{line:04}\n"))
        .collect::<String>();
    write_git_review_fixture_file(repo.path(), "fragment.txt", &original);
    write_git_review_fixture_file(repo.path(), "other.txt", "same\n");
    let base = commit_git_review_fixture(repo.path(), "fragment base");
    let changed = (0..600)
        .map(|line| format!("new-{line:04}\n"))
        .collect::<String>();
    write_git_review_fixture_file(repo.path(), "fragment.txt", &changed);
    let head = commit_git_review_fixture(repo.path(), "fragment head");
    let (raw_exit, raw_diff, raw_stderr) = run_command_full_capture(
        &format!("git diff --unified=80 {base} {head} -- fragment.txt"),
        repo.path(),
        30,
    );
    assert_eq!(raw_exit, 0, "raw committed diff failed: {raw_stderr}");
    let hunk_start = raw_diff.find("@@ ").expect("committed hunk header");
    let authoritative_hunk = raw_diff[hunk_start..].trim_end_matches('\n').to_string();

    let runtime = test_runtime();
    let client_id = "committed-fragment-continuation";
    let project =
        register_structured_git_agent_at_path(&runtime, client_id, "repo", repo.path()).await;
    let paths = Some(vec!["fragment.txt".to_string()]);
    let (first, _, first_scripts) = run_runner_git_diff_hunks_committed_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        paths.clone(),
        10,
        400,
        base.clone(),
        head.clone(),
        None,
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    assert_eq!(
        first.output["recovery"]["current_hunk"]["reason_code"],
        "hunk_fragment_continuation_available"
    );
    let fragment_token = first.output["recovery"]["current_hunk"]["next_call"]["arguments"]
        ["continuation"]
        .as_str()
        .expect("committed fragment token")
        .to_string();
    assert!(fragment_token.len() <= GIT_DIFF_HUNKS_CONTINUATION_MAX_BYTES);
    assert!(first_scripts
        .iter()
        .all(|script| !script.contains(&fragment_token)));

    let (path_mismatch, _, _) = run_runner_git_diff_hunks_committed_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        Some(vec!["other.txt".to_string()]),
        10,
        400,
        base.clone(),
        head.clone(),
        Some(fragment_token.clone()),
    )
    .await;
    assert!(!path_mismatch.success);
    assert_eq!(path_mismatch.output["reason_code"], "continuation_mismatch");

    let (range_mismatch, _, _) = run_runner_git_diff_hunks_committed_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        paths.clone(),
        10,
        400,
        head.clone(),
        head.clone(),
        Some(fragment_token.clone()),
    )
    .await;
    assert!(!range_mismatch.success);
    assert_eq!(
        range_mismatch.output["reason_code"],
        "continuation_mismatch"
    );

    {
        use base64::{engine::general_purpose, Engine as _};
        let encoded = fragment_token.strip_prefix("wcdh2.").unwrap();
        let mut decoded = general_purpose::URL_SAFE_NO_PAD.decode(encoded).unwrap();
        let fence_len = decoded[33] as usize;
        let line_last = 34 + fence_len + 8 + 7;
        decoded[line_last] ^= 1;
        let tampered = format!("wcdh2.{}", general_purpose::URL_SAFE_NO_PAD.encode(decoded));
        let (tampered_result, _, _) = run_runner_git_diff_hunks_committed_page(
            &runtime,
            client_id,
            &project,
            repo.path(),
            paths.clone(),
            10,
            400,
            base.clone(),
            head.clone(),
            Some(tampered),
        )
        .await;
        assert!(!tampered_result.success);
        assert_eq!(
            tampered_result.output["reason_code"],
            "invalid_continuation"
        );
    }

    let committed_as_worktree = runtime
        .git_diff_hunks_continued(
            project.clone(),
            paths.clone(),
            Some(10),
            Some(400),
            Some(false),
            Some(fragment_token.clone()),
        )
        .await;
    assert!(!committed_as_worktree.success);
    assert_eq!(
        committed_as_worktree.output["reason_code"],
        "continuation_mismatch"
    );
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());

    let mut reconstructed = first.output["files"][0]["hunks"][0]["diff"]
        .as_str()
        .unwrap()
        .to_string();
    let mut next_fragment = Some(fragment_token.clone());
    let mut fragment_steps = 0usize;
    while let Some(token) = next_fragment.take() {
        let (page, _, scripts) = run_runner_git_diff_hunks_committed_page(
            &runtime,
            client_id,
            &project,
            repo.path(),
            paths.clone(),
            10,
            400,
            base.clone(),
            head.clone(),
            Some(token.clone()),
        )
        .await;
        assert!(page.success, "{:?}", page.error);
        assert!(scripts.iter().all(|script| !script.contains(&token)));
        let hunk = &page.output["files"][0]["hunks"][0];
        assert_eq!(hunk["continued"], true);
        reconstructed.push('\n');
        reconstructed.push_str(hunk["diff"].as_str().unwrap());
        fragment_steps += 1;
        next_fragment = page
            .output
            .get("recovery")
            .and_then(|recovery| recovery.get("current_hunk"))
            .and_then(|omitted| omitted.get("next_call"))
            .and_then(|call| call.get("arguments"))
            .and_then(|arguments| arguments.get("continuation"))
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    assert!(fragment_steps >= 2);
    assert_eq!(reconstructed, authoritative_hunk);

    let dirty = (0..600)
        .map(|line| format!("dirty-{line:04}\n"))
        .collect::<String>();
    fs::write(repo.path().join("fragment.txt"), dirty).unwrap();
    let (worktree_first, _, _) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        paths.clone(),
        10,
        400,
        false,
        None,
    )
    .await;
    assert!(worktree_first.success, "{:?}", worktree_first.error);
    let worktree_fragment = worktree_first.output["recovery"]["current_hunk"]["next_call"]
        ["arguments"]["continuation"]
        .as_str()
        .expect("worktree fragment token")
        .to_string();
    let (worktree_as_committed, _, _) = run_runner_git_diff_hunks_committed_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        paths,
        10,
        400,
        base,
        head,
        Some(worktree_fragment),
    )
    .await;
    assert!(!worktree_as_committed.success);
    assert_eq!(
        worktree_as_committed.output["reason_code"],
        "continuation_mismatch"
    );
}

#[tokio::test]
async fn git_diff_hunks_committed_drains_bounded_consumer_and_preserves_producer_failure() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    write_git_review_fixture_file(tmp.path(), "file.txt", "old\n");
    let base = commit_git_review_fixture(tmp.path(), "base");
    write_git_review_fixture_file(tmp.path(), "file.txt", "new\n");
    let head = commit_git_review_fixture(tmp.path(), "head");
    let runtime = test_runtime();
    let project = register_structured_git_agent_at_path(
        &runtime,
        "committed-producer-failure",
        "repo",
        tmp.path(),
    )
    .await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let base = base.clone();
        let head = head.clone();
        async move {
            runtime
                .git_diff_hunks_continued_with_range(
                    project,
                    Some(vec!["file.txt".to_string()]),
                    Some(1),
                    Some(40),
                    None,
                    Some(base),
                    Some(head),
                    None,
                )
                .await
        }
    });

    let scope_request = wait_for_patch_agent_request(&runtime, "committed-producer-failure").await;
    complete_agent_request_by_running_locally(
        &runtime,
        "committed-producer-failure",
        scope_request,
    )
    .await;

    let mut page_request =
        wait_for_patch_agent_request(&runtime, "committed-producer-failure").await;
    let page_script = page_request
        .script
        .as_ref()
        .expect("typed page script")
        .script
        .clone();
    let head_q = shell_escape_simple(&head);
    let needle = format!(
        "git --no-pager -c core.quotePath=false diff --no-ext-diff --no-textconv --find-renames --unified=80 {} {head_q} -- 'file.txt'",
        shell_escape_simple(&base),
    );
    assert_eq!(
        page_script.matches(&needle).count(),
        1,
        "middle diff producer must be uniquely injectable"
    );
    let producer_body = concat!(
        "printf 'diff --git a/file.txt b/file.txt\\n--- a/file.txt\\n+++ b/file.txt\\n@@ -1 +1 @@\\n-old\\n'; ",
        "i=0; while [ \"$i\" -lt 120000 ]; do printf '+payload-%06d\\n' \"$i\"; i=$((i+1)); done; ",
        "exit 7"
    );
    let injected = format!("sh -c {}", shell_escape_simple(producer_body));
    page_request
        .script
        .as_mut()
        .expect("typed page script")
        .script = page_script.replacen(&needle, &injected, 1);
    let (exit_code, stdout, stderr) = run_runner_shell_request_locally(&page_request);
    assert_eq!(
        exit_code, 0,
        "page envelope should carry producer failure structurally: {stderr}"
    );
    assert!(
        stdout.contains("diff_exit=7\n"),
        "bounded consumer must drain to the producer's real exit status, not SIGPIPE: {stdout}"
    );
    assert!(!stdout.contains("diff_exit=141\n"));
    assert!(
        stdout.len() < 48 * 1024,
        "consumer must retain only a bounded page despite multi-megabyte producer output"
    );
    complete_patch_agent_request(
        &runtime,
        "committed-producer-failure",
        &page_request.request_id,
        exit_code,
        &stdout,
        &stderr,
    )
    .await;
    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["reason_code"], "git_diff_failed");
}

fn git_test_command_ok(repo: &Path, command: &str) {
    let (exit_code, stdout, stderr, _) = run_command_sync(command, repo, 30);
    assert_eq!(
        exit_code, 0,
        "{command} failed: stdout={stdout} stderr={stderr}"
    );
}

fn assert_git_diff_hunks_sparse_recovery_calls_parse(recovery: &Value) {
    assert!(recovery
        .as_object()
        .unwrap()
        .keys()
        .all(|key| matches!(key.as_str(), "current_hunk" | "later_hunks")));
    for lane in ["current_hunk", "later_hunks"] {
        if let Some(value) = recovery.get(lane) {
            assert!(value
                .as_object()
                .unwrap()
                .keys()
                .all(|key| key == "next_call" || (lane == "current_hunk" && key == "reason_code")));
            if let Some(call) = value.get("next_call") {
                assert_eq!(call["follow_up_kind"], "mechanically_followable");
                webcodex_tool_contracts::test_support::validate_generated_tool_call_against_registered_input_schema(call)
                    .expect("read_git_diff_hunks recovery next_call must pass registered inputSchema");
                ToolCall::from_tool_name(call["tool"].as_str().unwrap(), call["arguments"].clone())
                    .expect("each recovery lane must parse directly");
            }
        }
    }
}

fn assert_git_diff_hunks_recovery_call_parses(recovery: &Value) {
    assert_eq!(recovery["follow_up_kind"], "mechanically_followable");
    webcodex_tool_contracts::test_support::validate_generated_tool_call_against_registered_input_schema(recovery)
        .expect("read_git_diff_hunks generated follow-up must pass registered inputSchema");
    let tool = recovery["tool"]
        .as_str()
        .expect("recovery tool must be a string");
    ToolCall::from_tool_name(tool, recovery["arguments"].clone())
        .expect("structured git diff recovery call must parse directly");
}

#[tokio::test]
async fn git_diff_hunks_stable_multi_page_traversal_has_no_duplicate_or_missing_records() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    let original = (0..500)
        .map(|line| format!("line-{line:03}\n"))
        .collect::<String>();
    for file in 0..3 {
        commit_file(
            repo.path(),
            &format!("file-{file}.txt"),
            &original,
            &format!("add file {file}"),
        );
    }
    for file in 0..3 {
        let changed = (0..500)
            .map(|line| {
                if matches!(line, 10 | 210 | 410) {
                    format!("changed-{file}-{line:03}\n")
                } else {
                    format!("line-{line:03}\n")
                }
            })
            .collect::<String>();
        fs::write(repo.path().join(format!("file-{file}.txt")), changed).unwrap();
    }

    let runtime = test_runtime();
    let client_id = "diff-hunks-pages";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let mut continuation = None;
    let mut logical_files = Vec::new();
    let mut logical_hunks = Vec::new();
    let mut seen_tokens = HashSet::new();
    let mut finished = false;

    for _ in 0..10 {
        let prior_token = continuation.clone();
        let (result, _stdout_bytes, command) = run_runner_git_diff_hunks_page(
            &runtime,
            client_id,
            &project,
            repo.path(),
            None,
            2,
            400,
            false,
            continuation,
        )
        .await;
        assert!(result.success, "{:?}", result.error);
        if let Some(prior_token) = prior_token.as_deref() {
            assert!(
                !command.contains(prior_token),
                "opaque continuation must not be interpolated into the shell command"
            );
        }
        for file in result.output["files"].as_array().unwrap() {
            let path = file["path"].as_str().unwrap().to_string();
            if file.get("continued").and_then(Value::as_bool) != Some(true) {
                logical_files.push(path.clone());
            }
            for hunk in file["hunks"].as_array().unwrap() {
                logical_hunks.push(format!("{}|{}", path, hunk["header"].as_str().unwrap()));
            }
        }
        if result.output["has_more"] == false {
            assert!(result.output["recovery"].get("later_hunks").is_none());
            finished = true;
            break;
        }
        let next = result.output["recovery"]["later_hunks"]["next_call"]["arguments"]
            ["continuation"]
            .as_str()
            .expect("non-final page continuation")
            .to_string();
        let recovery = &result.output["recovery"];
        assert!(recovery["later_hunks"].is_object());
        assert!(recovery["later_hunks"]["next_call"].is_object());
        assert_eq!(
            recovery["later_hunks"]["next_call"]["arguments"]["project"],
            project
        );
        assert_eq!(
            recovery["later_hunks"]["next_call"]["arguments"]["paths"],
            json!([])
        );
        assert_eq!(
            recovery["later_hunks"]["next_call"]["arguments"]["cached"],
            false
        );
        assert_eq!(
            recovery["later_hunks"]["next_call"]["arguments"]["max_hunks"],
            2
        );
        assert_eq!(
            recovery["later_hunks"]["next_call"]["arguments"]["max_hunk_lines"],
            400
        );
        assert_eq!(
            recovery["later_hunks"]["next_call"]["arguments"]["continuation"],
            next
        );
        assert_git_diff_hunks_sparse_recovery_calls_parse(recovery);
        assert!(
            seen_tokens.insert(next.clone()),
            "continuation did not advance"
        );
        continuation = Some(next);
    }

    assert!(finished, "multi-page traversal did not terminate");
    assert_eq!(
        logical_files,
        vec!["file-0.txt", "file-1.txt", "file-2.txt"]
    );
    assert_eq!(
        logical_hunks.len(),
        9,
        "unexpected hunk traversal: {logical_hunks:?}"
    );
    assert_eq!(
        logical_hunks.iter().collect::<HashSet<_>>().len(),
        logical_hunks.len(),
        "duplicate logical hunk returned across pages"
    );
}

#[tokio::test]
async fn git_diff_hunks_large_raw_diff_is_bounded_before_runner_result_retention() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    let original = (0..2500)
        .map(|line| format!("original-{line:04}-{}\n", "x".repeat(72)))
        .collect::<String>();
    for file in 0..2 {
        fs::write(repo.path().join(format!("large-{file}.txt")), &original).unwrap();
    }
    git_test_command_ok(repo.path(), "git add -- . && git commit -m large-baseline");
    for file in 0..2 {
        let changed = (0..2500)
            .map(|line| format!("changed-{file}-{line:04}-{}\n", "y".repeat(72)))
            .collect::<String>();
        fs::write(repo.path().join(format!("large-{file}.txt")), changed).unwrap();
    }
    let (raw_exit, raw_diff, raw_stderr) =
        run_command_full_capture("git diff --unified=80", repo.path(), 30);
    assert_eq!(raw_exit, 0, "raw git diff failed: {raw_stderr}");
    assert!(
        raw_diff.len() > ORDINARY_RUNNER_RESULT_RETENTION_COMPAT_BYTES,
        "fixture did not exceed ordinary Runner result-retention compatibility floor: {} bytes",
        raw_diff.len()
    );

    assert_eq!(
        DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES, MAX_GIT_DIFF_HUNKS_PAGE_BYTES,
        "default Git review page should use the full safe Runner-retention budget"
    );

    let runtime = test_runtime();
    let client_id = "diff-hunks-large";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let (result, runner_stdout_bytes, _command) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        None,
        1,
        12,
        false,
        None,
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(
        result.output["max_page_bytes"],
        DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES
    );
    assert!(
        runner_stdout_bytes <= DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES + 4096,
        "producer sent {runner_stdout_bytes} bytes beyond default page-retention headroom"
    );
    assert_eq!(result.output["files"][0]["path"], "large-0.txt");
    assert_eq!(result.output["has_more"], true);
    assert!(
        result.output["recovery"]["later_hunks"]["next_call"]["arguments"]["continuation"]
            .as_str()
            .is_some()
    );
    let reasons = result.output["truncation_reasons"].as_array().unwrap();
    assert!(!reasons.iter().any(|reason| reason == "page_hunk_limit"));
    assert!(reasons.iter().any(|reason| reason == "hunk_line_limit"));
    assert!(!reasons.iter().any(|reason| reason == "page_byte_budget"));
    let recovery = &result.output["recovery"];
    assert!(recovery["current_hunk"].is_object());
    assert!(recovery["current_hunk"]["next_call"].is_object());
    assert_eq!(
        recovery["current_hunk"]["reason_code"],
        "hunk_fragment_continuation_available"
    );
    assert!(recovery["later_hunks"]["next_call"].is_object());
    assert_ne!(
        recovery["current_hunk"]["next_call"]["arguments"]["continuation"],
        recovery["later_hunks"]["next_call"]["arguments"]["continuation"]
    );
    assert_eq!(
        recovery["current_hunk"]["next_call"]["arguments"]["paths"],
        json!([])
    );
    assert_eq!(
        recovery["current_hunk"]["next_call"]["arguments"]["max_hunk_lines"],
        12
    );
    assert_eq!(
        recovery["current_hunk"]["next_call"]["arguments"]["max_page_bytes"],
        DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES
    );
    assert_eq!(
        recovery["later_hunks"]["next_call"]["arguments"]["max_hunk_lines"],
        12
    );

    assert_git_diff_hunks_sparse_recovery_calls_parse(recovery);
    assert!(
        serde_json::to_vec(&result).unwrap().len() <= MODEL_INSPECTION_MAX_RESULT_BYTES,
        "serialized result exceeded explicit model-inspection ceiling"
    );
}

#[tokio::test]
async fn git_diff_hunks_page_budget_is_configurable_bounded_and_scope_bound() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    for file in 0..40 {
        let body = (0..30)
            .map(|line| format!("base-{file:02}-{line:02}-{}\n", "x".repeat(72)))
            .collect::<String>();
        fs::write(repo.path().join(format!("page-{file:02}.txt")), body).unwrap();
    }
    git_test_command_ok(repo.path(), "git add -- . && git commit -m page-baseline");
    for file in 0..40 {
        let body = (0..30)
            .map(|line| format!("changed-{file:02}-{line:02}-{}\n", "y".repeat(72)))
            .collect::<String>();
        fs::write(repo.path().join(format!("page-{file:02}.txt")), body).unwrap();
    }

    let runtime = test_runtime();
    let client_id = "diff-hunks-page-budget";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let (small, small_stdout_bytes, _) = run_runner_git_diff_hunks_page_with_budget(
        &runtime,
        client_id,
        &project,
        repo.path(),
        None,
        100,
        100,
        Some(1),
        false,
        None,
    )
    .await;
    assert!(small.success, "{:?}", small.error);
    assert_eq!(
        small.output["max_page_bytes"],
        MIN_GIT_DIFF_HUNKS_PAGE_BYTES
    );
    assert!(small_stdout_bytes <= MIN_GIT_DIFF_HUNKS_PAGE_BYTES + 4096);
    assert_eq!(small.output["has_more"], true);
    let token = small.output["recovery"]["later_hunks"]["next_call"]["arguments"]["continuation"]
        .as_str()
        .expect("small producer page must continue")
        .to_string();
    assert_eq!(
        small.output["recovery"]["later_hunks"]["next_call"]["arguments"]["max_page_bytes"],
        MIN_GIT_DIFF_HUNKS_PAGE_BYTES
    );

    let mismatch = runtime
        .git_diff_hunks_continued_with_range_and_page_bytes(
            project.clone(),
            None,
            Some(100),
            Some(100),
            Some(DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES),
            Some(false),
            None,
            None,
            Some(token),
        )
        .await;
    assert!(!mismatch.success);
    assert_eq!(mismatch.output["reason_code"], "continuation_mismatch");
    assert!(
        probe_patch_agent_request(&runtime, client_id)
            .await
            .is_none(),
        "page-budget scope mismatch must stop before producer dispatch"
    );

    let (large, large_stdout_bytes, _) = run_runner_git_diff_hunks_page_with_budget(
        &runtime,
        client_id,
        &project,
        repo.path(),
        None,
        100,
        100,
        Some(MAX_GIT_DIFF_HUNKS_PAGE_BYTES + 64 * 1024),
        false,
        None,
    )
    .await;
    assert!(large.success, "{:?}", large.error);
    assert_eq!(
        large.output["max_page_bytes"],
        MAX_GIT_DIFF_HUNKS_PAGE_BYTES
    );
    assert!(large_stdout_bytes <= MAX_GIT_DIFF_HUNKS_PAGE_BYTES + 4096);
    assert!(
        large.output["hunk_count"].as_u64().unwrap() > small.output["hunk_count"].as_u64().unwrap(),
        "larger producer page should return more complete hunk records"
    );
    assert!(
        serde_json::to_vec(&large).unwrap().len() <= MODEL_INSPECTION_MAX_RESULT_BYTES,
        "producer page and final model-facing ceiling must remain independently bounded"
    );
}

#[tokio::test]
async fn git_diff_hunks_worktree_continuation_fails_stale_after_relevant_change() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    commit_file(repo.path(), "a.txt", "a0\n", "add a");
    commit_file(repo.path(), "b.txt", "b0\n", "add b");
    fs::write(repo.path().join("a.txt"), "a1\n").unwrap();
    fs::write(repo.path().join("b.txt"), "b1\n").unwrap();

    let runtime = test_runtime();
    let client_id = "diff-hunks-worktree-stale";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let (page, _, _) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        None,
        1,
        40,
        false,
        None,
    )
    .await;
    assert!(page.success, "{:?}", page.error);
    let token = page.output["recovery"]["later_hunks"]["next_call"]["arguments"]["continuation"]
        .as_str()
        .expect("first page continuation")
        .to_string();
    fs::write(repo.path().join("b.txt"), "b2\nextra\n").unwrap();

    let (stale, _, _) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        None,
        1,
        40,
        false,
        Some(token),
    )
    .await;
    assert!(!stale.success);
    assert_eq!(stale.output["reason_code"], "stale_continuation");
    assert_eq!(stale.output["files"], json!([]));
    assert!(stale.output["recovery"].get("later_hunks").is_none());
    assert_eq!(stale.output["state_changed"], false);
}

#[tokio::test]
async fn git_diff_hunks_cached_continuation_fails_stale_after_index_change() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    commit_file(repo.path(), "a.txt", "a0\n", "add a");
    commit_file(repo.path(), "b.txt", "b0\n", "add b");
    fs::write(repo.path().join("a.txt"), "a1\n").unwrap();
    fs::write(repo.path().join("b.txt"), "b1\n").unwrap();
    git_test_command_ok(repo.path(), "git add -- a.txt b.txt");

    let runtime = test_runtime();
    let client_id = "diff-hunks-cached-stale";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let (page, _, _) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        None,
        1,
        40,
        true,
        None,
    )
    .await;
    assert!(page.success, "{:?}", page.error);
    let token = page.output["recovery"]["later_hunks"]["next_call"]["arguments"]["continuation"]
        .as_str()
        .expect("cached continuation")
        .to_string();
    fs::write(repo.path().join("b.txt"), "b2\n").unwrap();
    git_test_command_ok(repo.path(), "git add -- b.txt");

    let (stale, _, _) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        None,
        1,
        40,
        true,
        Some(token),
    )
    .await;
    assert!(!stale.success);
    assert_eq!(stale.output["reason_code"], "stale_continuation");
    assert_eq!(stale.output["files"], json!([]));
    assert!(stale.output["recovery"].get("later_hunks").is_none());
}

#[tokio::test]
async fn git_diff_hunks_scoped_fence_ignores_outside_change_and_rejects_scope_mismatch() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    commit_file(repo.path(), "a.txt", "a0\n", "add a");
    commit_file(repo.path(), "b.txt", "b0\n", "add b");
    commit_file(repo.path(), "outside.txt", "outside0\n", "add outside");
    fs::write(repo.path().join("a.txt"), "a1\n").unwrap();
    fs::write(repo.path().join("b.txt"), "b1\n").unwrap();
    let scope = Some(vec!["a.txt".to_string(), "b.txt".to_string()]);

    let runtime = test_runtime();
    let client_id = "diff-hunks-scoped";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let (page, _, _) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        scope.clone(),
        1,
        40,
        false,
        None,
    )
    .await;
    assert!(page.success, "{:?}", page.error);
    let token = page.output["recovery"]["later_hunks"]["next_call"]["arguments"]["continuation"]
        .as_str()
        .expect("scoped continuation")
        .to_string();
    fs::write(repo.path().join("outside.txt"), "outside1\n").unwrap();

    let (continued, _, command) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        Some(vec!["b.txt".to_string(), "a.txt".to_string()]),
        1,
        40,
        false,
        Some(token.clone()),
    )
    .await;
    assert!(continued.success, "{:?}", continued.error);
    assert!(
        !command.contains(&token),
        "opaque continuation content reached the shell command"
    );

    let other_client_id = "diff-hunks-scoped-other";
    let other_project =
        register_runner_project_at_path(&runtime, other_client_id, "repo", repo.path()).await;
    let project_mismatch = runtime
        .git_diff_hunks_continued(
            other_project,
            scope.clone(),
            Some(1),
            Some(40),
            Some(false),
            Some(token.clone()),
        )
        .await;
    assert!(!project_mismatch.success);
    assert_eq!(
        project_mismatch.output["reason_code"],
        "continuation_mismatch"
    );
    assert!(probe_patch_agent_request(&runtime, other_client_id)
        .await
        .is_none());

    let mismatch = runtime
        .git_diff_hunks_continued(
            project.clone(),
            Some(vec!["a.txt".to_string()]),
            Some(1),
            Some(40),
            Some(false),
            Some(token.clone()),
        )
        .await;
    assert!(!mismatch.success);
    assert_eq!(mismatch.output["reason_code"], "continuation_mismatch");
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());

    let cached_mismatch = runtime
        .git_diff_hunks_continued(project, scope, Some(1), Some(40), Some(true), Some(token))
        .await;
    assert!(!cached_mismatch.success);
    assert_eq!(
        cached_mismatch.output["reason_code"],
        "continuation_mismatch"
    );
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
}

#[tokio::test]
async fn git_diff_hunks_worktree_hunk_fragment_continuation_fails_stale_after_scoped_change() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    let original = (0..600)
        .map(|line| format!("old-{line:04}\n"))
        .collect::<String>();
    commit_file(repo.path(), "a.txt", &original, "fragment baseline");
    let changed = (0..600)
        .map(|line| format!("new-{line:04}\n"))
        .collect::<String>();
    fs::write(repo.path().join("a.txt"), &changed).unwrap();

    let runtime = test_runtime();
    let client_id = "diff-hunks-fragment-worktree-stale";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let scope = Some(vec!["a.txt".to_string()]);
    let (first, _, _) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        scope.clone(),
        10,
        400,
        false,
        None,
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let token = first.output["recovery"]["current_hunk"]["next_call"]["arguments"]["continuation"]
        .as_str()
        .expect("fragment continuation")
        .to_string();
    fs::write(repo.path().join("a.txt"), format!("{changed}new-tail\n")).unwrap();

    let (stale, _, script) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        scope,
        10,
        400,
        false,
        Some(token.clone()),
    )
    .await;
    assert!(!stale.success);
    assert_eq!(stale.output["reason_code"], "stale_continuation");
    assert_eq!(stale.output["files"], json!([]));
    assert!(stale.output["recovery"].get("later_hunks").is_none());
    assert!(!script.contains(&token));
}

#[tokio::test]
async fn git_diff_hunks_cached_hunk_fragment_continuation_fails_stale_after_index_change() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    let original = (0..600)
        .map(|line| format!("old-{line:04}\n"))
        .collect::<String>();
    commit_file(repo.path(), "a.txt", &original, "fragment baseline");
    let changed = (0..600)
        .map(|line| format!("new-{line:04}\n"))
        .collect::<String>();
    fs::write(repo.path().join("a.txt"), &changed).unwrap();
    git_test_command_ok(repo.path(), "git add -- a.txt");

    let runtime = test_runtime();
    let client_id = "diff-hunks-fragment-cached-stale";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let scope = Some(vec!["a.txt".to_string()]);
    let (first, _, _) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        scope.clone(),
        10,
        400,
        true,
        None,
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let token = first.output["recovery"]["current_hunk"]["next_call"]["arguments"]["continuation"]
        .as_str()
        .expect("cached fragment continuation")
        .to_string();
    fs::write(repo.path().join("a.txt"), format!("{changed}new-tail\n")).unwrap();
    git_test_command_ok(repo.path(), "git add -- a.txt");

    let (stale, _, _) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        scope,
        10,
        400,
        true,
        Some(token),
    )
    .await;
    assert!(!stale.success);
    assert_eq!(stale.output["reason_code"], "stale_continuation");
    assert_eq!(stale.output["files"], json!([]));
}

#[tokio::test]
async fn git_diff_hunks_scoped_hunk_fragment_continuation_ignores_outside_change() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    let original = (0..600)
        .map(|line| format!("old-{line:04}\n"))
        .collect::<String>();
    commit_file(repo.path(), "a.txt", &original, "fragment baseline");
    commit_file(repo.path(), "outside.txt", "outside0\n", "outside baseline");
    let changed = (0..600)
        .map(|line| format!("new-{line:04}\n"))
        .collect::<String>();
    fs::write(repo.path().join("a.txt"), changed).unwrap();

    let runtime = test_runtime();
    let client_id = "diff-hunks-fragment-scoped-outside";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let scope = Some(vec!["a.txt".to_string()]);
    let (first, _, _) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        scope.clone(),
        10,
        400,
        false,
        None,
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let token = first.output["recovery"]["current_hunk"]["next_call"]["arguments"]["continuation"]
        .as_str()
        .expect("scoped fragment continuation")
        .to_string();
    fs::write(repo.path().join("outside.txt"), "outside1\n").unwrap();

    let (continued, _, script) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        scope,
        10,
        400,
        false,
        Some(token.clone()),
    )
    .await;
    assert!(continued.success, "{:?}", continued.error);
    assert_eq!(continued.output["files"][0]["path"], "a.txt");
    assert_eq!(continued.output["files"][0]["hunks"][0]["continued"], true);
    assert!(!script.contains(&token));
}

#[tokio::test]
async fn git_diff_hunks_binary_records_advance_across_byte_bounded_pages() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    let file_count = 120usize;
    let names = (0..file_count)
        .map(|index| format!("binary-{index:03}-{}.bin", "x".repeat(96)))
        .collect::<Vec<_>>();
    for (index, name) in names.iter().enumerate() {
        let mut bytes = vec![0u8; 512];
        bytes[1] = index as u8;
        fs::write(repo.path().join(name), bytes).unwrap();
    }
    git_test_command_ok(
        repo.path(),
        "git add -- . && git commit -q -m binary-baseline",
    );
    for (index, name) in names.iter().enumerate() {
        let mut bytes = vec![0u8; 512];
        bytes[1] = index as u8;
        bytes[2] = 1;
        fs::write(repo.path().join(name), bytes).unwrap();
    }

    let runtime = test_runtime();
    let client_id = "diff-hunks-binary";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let mut continuation = None;
    let mut returned = Vec::new();
    let mut finished = false;
    for _ in 0..10 {
        let (page, runner_stdout_bytes, _) = run_runner_git_diff_hunks_page(
            &runtime,
            client_id,
            &project,
            repo.path(),
            None,
            1,
            20,
            false,
            continuation,
        )
        .await;
        assert!(page.success, "{:?}", page.error);
        assert_eq!(page.output["hunk_count"], 0);
        assert!(runner_stdout_bytes <= DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES + 4096);
        for file in page.output["files"].as_array().unwrap() {
            assert_ne!(file.get("continued").and_then(Value::as_bool), Some(true));
            returned.push(file["path"].as_str().unwrap().to_string());
        }
        if page.output["has_more"] == false {
            assert!(page.output["recovery"].get("later_hunks").is_none());
            finished = true;
            break;
        }
        assert!(page.output["truncation_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason == "page_byte_budget"));
        let recovery = &page.output["recovery"];
        assert!(recovery["later_hunks"].is_object());
        assert_eq!(
            recovery["later_hunks"]["next_call"]["arguments"]["cached"],
            false
        );
        assert_eq!(
            recovery["later_hunks"]["next_call"]["arguments"]["max_hunks"],
            1
        );
        assert_eq!(
            recovery["later_hunks"]["next_call"]["arguments"]["max_hunk_lines"],
            20
        );

        assert_git_diff_hunks_sparse_recovery_calls_parse(recovery);
        continuation = Some(
            page.output["recovery"]["later_hunks"]["next_call"]["arguments"]["continuation"]
                .as_str()
                .unwrap()
                .to_string(),
        );
    }
    assert!(finished, "binary pagination did not terminate");
    assert_eq!(returned.len(), file_count);
    assert_eq!(returned.iter().collect::<HashSet<_>>().len(), file_count);
    assert_eq!(returned, names);
}

#[tokio::test]
async fn git_diff_hunks_hunk_line_limit_does_not_create_fake_continuation() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    let original = (0..80)
        .map(|line| format!("old-{line:03}\n"))
        .collect::<String>();
    commit_file(repo.path(), "long.txt", &original, "add long");
    let changed = (0..80)
        .map(|line| format!("new-{line:03}\n"))
        .collect::<String>();
    fs::write(repo.path().join("long.txt"), changed).unwrap();

    let runtime = test_runtime();
    let client_id = "diff-hunks-line-limit";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let (page, _, _) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        None,
        10,
        5,
        false,
        None,
    )
    .await;
    assert!(page.success, "{:?}", page.error);
    assert_eq!(page.output["hunk_count"], 1);
    assert_eq!(page.output["has_more"], false);
    assert!(page.output["recovery"].get("later_hunks").is_none());
    assert_eq!(page.output["files"][0]["hunks"][0]["truncated"], true);
    assert!(page.output["truncation_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason == "hunk_line_limit"));
    assert!(!page.output["truncation_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason == "page_hunk_limit"));
    let recovery = &page.output["recovery"];
    assert!(recovery["current_hunk"].is_object());
    assert!(recovery.get("later_hunks").is_none());
    assert!(recovery["current_hunk"]["next_call"].is_object());
    assert_eq!(
        recovery["current_hunk"]["reason_code"],
        "larger_max_hunk_lines_available"
    );
    assert_eq!(
        recovery["current_hunk"]["next_call"]["arguments"]["paths"],
        json!(["long.txt"])
    );
    assert_eq!(
        recovery["current_hunk"]["next_call"]["arguments"]["max_hunk_lines"],
        400
    );
    assert!(recovery["current_hunk"]["next_call"]["arguments"]
        .get("continuation")
        .is_none());
    assert_git_diff_hunks_sparse_recovery_calls_parse(recovery);
    let recovered = run_parser_ready_worktree_git_diff_hunks_call(
        &runtime,
        client_id,
        repo.path(),
        &recovery["current_hunk"]["next_call"],
    )
    .await;
    assert!(recovered.success, "{:?}", recovered.error);
    assert_eq!(recovered.output["truncated"], false);
    assert_eq!(recovered.output["has_more"], false);
    assert!(recovered.output["recovery"].get("later_hunks").is_none());
    assert!(recovered.output.get("recovery").is_none());
    assert_eq!(recovered.output["files"][0]["hunks"][0]["truncated"], false);
    assert!(recovered.output["files"][0]["hunks"][0]["diff"]
        .as_str()
        .unwrap()
        .contains("+new-079"));
    let mut projected = ToolResult::ok(page.output.clone());
    sparsify_complete_git_review_success("read_git_diff_hunks", &mut projected);
    assert_eq!(
        projected.output, page.output,
        "truncated recovery evidence must never be sparsified"
    );
}

#[tokio::test]
async fn git_diff_hunks_hunk_fragment_continuation_reconstructs_over_400_line_hunk() {
    assert_eq!(MAX_MAX_HUNK_LINES, 400, "hard hunk-line ceiling changed");
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    let original = (0..600)
        .map(|line| format!("old-{line:04}\n"))
        .collect::<String>();
    commit_file(repo.path(), "fragment.txt", &original, "fragment baseline");
    let changed = (0..600)
        .map(|line| format!("new-{line:04}\n"))
        .collect::<String>();
    fs::write(repo.path().join("fragment.txt"), changed).unwrap();
    let (raw_exit, raw_diff, raw_stderr) =
        run_command_full_capture("git diff --unified=80 -- fragment.txt", repo.path(), 30);
    assert_eq!(raw_exit, 0, "raw diff failed: {raw_stderr}");
    let hunk_start = raw_diff.find("@@ ").expect("authoritative hunk header");
    let authoritative_hunk = raw_diff[hunk_start..].trim_end_matches('\n').to_string();
    assert!(authoritative_hunk.lines().count() > 400);

    let runtime = test_runtime();
    let client_id = "diff-hunks-fragment-reconstruct";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let scope = Some(vec!["fragment.txt".to_string()]);
    let (first, _, _) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        scope.clone(),
        10,
        400,
        false,
        None,
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    assert_eq!(first.output["hunk_count"], 1);
    assert_eq!(first.output["files"][0]["hunks"][0]["truncated"], true);
    assert!(
        first.output["files"][0]["hunks"][0]["diff"]
            .as_str()
            .unwrap()
            .lines()
            .count()
            <= MAX_MAX_HUNK_LINES
    );
    let recovery = &first.output["recovery"];
    assert!(recovery["current_hunk"]["next_call"].is_object());
    assert_eq!(
        recovery["current_hunk"]["reason_code"],
        "hunk_fragment_continuation_available"
    );
    assert_eq!(
        recovery["current_hunk"]["next_call"]["tool"],
        "read_git_diff_hunks"
    );
    assert_eq!(
        recovery["current_hunk"]["next_call"]["arguments"]["paths"],
        json!(["fragment.txt"])
    );
    assert_eq!(
        recovery["current_hunk"]["next_call"]["arguments"]["max_hunk_lines"],
        400
    );
    assert_git_diff_hunks_sparse_recovery_calls_parse(recovery);

    let header = first.output["files"][0]["hunks"][0]["header"]
        .as_str()
        .unwrap()
        .to_string();
    let mut reconstructed = first.output["files"][0]["hunks"][0]["diff"]
        .as_str()
        .unwrap()
        .to_string();
    let mut current = first;
    let mut fragment_count = 0usize;
    for _ in 0..10 {
        let Some(current_recovery) = current.output.get("recovery") else {
            break;
        };
        if current_recovery.get("current_hunk").is_none() {
            break;
        }
        assert!(current_recovery["current_hunk"]["next_call"].is_object());
        assert_eq!(
            current_recovery["current_hunk"]["reason_code"],
            "hunk_fragment_continuation_available"
        );
        let next_call = &current_recovery["current_hunk"]["next_call"];
        let parsed = ToolCall::from_tool_name(
            next_call["tool"].as_str().unwrap(),
            next_call["arguments"].clone(),
        )
        .expect("fragment next_call must be parser-ready");
        assert!(matches!(parsed, ToolCall::GitDiffHunks { .. }));
        let token = next_call["arguments"]["continuation"]
            .as_str()
            .expect("fragment token")
            .to_string();
        assert!(token.len() <= GIT_DIFF_HUNKS_CONTINUATION_MAX_BYTES);
        let (next, _, script) = run_runner_git_diff_hunks_page(
            &runtime,
            client_id,
            &project,
            repo.path(),
            scope.clone(),
            10,
            400,
            false,
            Some(token.clone()),
        )
        .await;
        assert!(next.success, "{:?}", next.error);
        assert!(
            !script.contains(&token),
            "opaque fragment token reached the producer shell"
        );
        let hunk = &next.output["files"][0]["hunks"][0];
        assert_eq!(hunk["continued"], true);
        assert_eq!(hunk["header"], header);
        let body = hunk["diff"].as_str().expect("continued hunk body");
        assert!(!body.is_empty());
        assert!(
            !body.starts_with("@@ "),
            "continued body replayed hunk header"
        );
        assert!(body.lines().count() < MAX_MAX_HUNK_LINES);
        reconstructed.push('\n');
        reconstructed.push_str(body);
        fragment_count += 1;
        current = next;
    }
    assert!(
        fragment_count >= 2,
        "fixture did not require multiple fragments"
    );
    assert_eq!(reconstructed, authoritative_hunk);
    assert_eq!(current.output["has_more"], false);
    assert!(current.output["recovery"].get("later_hunks").is_none());
    assert!(current.output.get("recovery").is_none());
}

#[tokio::test]
async fn git_diff_hunks_complete_model_projection_keeps_scope_and_drops_derived_metadata() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    commit_file(repo.path(), "a.txt", "before\n", "baseline");
    fs::write(repo.path().join("a.txt"), "after\n").unwrap();

    let runtime = test_runtime();
    let client_id = "diff-hunks-complete-projection";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let (page, _, _) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        Some(vec!["a.txt".to_string()]),
        10,
        400,
        false,
        None,
    )
    .await;
    assert!(page.success, "{:?}", page.error);
    assert_eq!(page.output["truncated"], false);
    assert_eq!(page.output["has_more"], false);
    assert!(page.output["recovery"].get("later_hunks").is_none());
    assert!(page.output.get("recovery").is_none());
    assert!(page.output["files"][0]["hunks"][0]
        .get("line_count")
        .is_some());

    let canonical = page.output.clone();
    let mut projected = ToolResult::ok(canonical.clone());
    sparsify_complete_git_review_success("read_git_diff_hunks", &mut projected);
    let output = &projected.output;
    assert_eq!(output["project"], project);
    assert_eq!(output["paths"], json!(["a.txt"]));
    assert_eq!(output["cached"], false);
    assert!(output.get("files").is_some());
    for omitted in [
        "hunk_count",
        "truncated",
        "truncation_reasons",
        "has_more",
        "exit_code",
        "stderr",
    ] {
        assert!(
            output.get(omitted).is_none(),
            "derived {omitted} leaked: {output}"
        );
    }
    assert!(output["files"][0].get("old_path").is_none());
    assert!(output["files"][0]["hunks"][0].get("line_count").is_none());
    assert!(output["files"][0]["hunks"][0].get("truncated").is_none());
    assert!(canonical.get("hunk_count").is_some());
    assert!(canonical.get("truncated").is_some());
}

#[tokio::test]
async fn git_diff_hunks_page_recovery_replays_cached_path_filtered_scope() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    let original = (0..500)
        .map(|line| format!("line-{line:03}\n"))
        .collect::<String>();
    for name in ["a.txt", "b.txt"] {
        commit_file(repo.path(), name, &original, "baseline");
        let changed = (0..500)
            .map(|line| {
                if matches!(line, 10 | 210 | 410) {
                    format!("changed-{name}-{line:03}\n")
                } else {
                    format!("line-{line:03}\n")
                }
            })
            .collect::<String>();
        fs::write(repo.path().join(name), changed).unwrap();
    }
    git_test_command_ok(repo.path(), "git add -- a.txt b.txt");

    let runtime = test_runtime();
    let client_id = "diff-hunks-cached-scoped-recovery";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let (page, _, _) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        Some(vec!["a.txt".to_string()]),
        1,
        400,
        true,
        None,
    )
    .await;
    assert!(page.success, "{:?}", page.error);
    assert_eq!(page.output["files"][0]["path"], "a.txt");
    assert_eq!(page.output["has_more"], true);
    let recovery = &page.output["recovery"];
    assert!(recovery["later_hunks"].is_object());
    assert_eq!(
        recovery["later_hunks"]["next_call"]["arguments"]["project"],
        project
    );
    assert_eq!(
        recovery["later_hunks"]["next_call"]["arguments"]["cached"],
        true
    );
    assert_eq!(
        recovery["later_hunks"]["next_call"]["arguments"]["paths"],
        json!(["a.txt"])
    );
    assert_eq!(
        recovery["later_hunks"]["next_call"]["arguments"]["max_hunks"],
        1
    );
    assert_eq!(
        recovery["later_hunks"]["next_call"]["arguments"]["max_hunk_lines"],
        400
    );

    assert!(recovery["later_hunks"]["next_call"]["arguments"]
        .get("base_commit")
        .is_none());
    assert!(recovery["later_hunks"]["next_call"]["arguments"]
        .get("head_commit")
        .is_none());
    assert_git_diff_hunks_sparse_recovery_calls_parse(recovery);
}

#[tokio::test]
async fn git_diff_hunks_unrepresentable_final_hunk_fails_closed_after_record_locator() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    let old = format!("old-{}\n", "x".repeat(96 * 1024));
    commit_file(repo.path(), "huge.txt", &old, "baseline");
    let new = format!("new-{}\n", "y".repeat(96 * 1024));
    fs::write(repo.path().join("huge.txt"), new).unwrap();

    let runtime = test_runtime();
    let client_id = "diff-hunks-final-record-byte-truncation";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let (page, _, _) = run_runner_git_diff_hunks_page_with_budget(
        &runtime,
        client_id,
        &project,
        repo.path(),
        None,
        10,
        400,
        Some(MIN_GIT_DIFF_HUNKS_PAGE_BYTES),
        false,
        None,
    )
    .await;

    assert!(page.success, "{:?}", page.error);
    assert_eq!(page.output["has_more"], true);
    assert_eq!(page.output["hunk_count"], 0);
    assert_eq!(
        page.output["truncation_reasons"],
        json!(["page_byte_budget"])
    );
    assert!(page.output["recovery"].get("current_hunk").is_none());
    let later_call = &page.output["recovery"]["later_hunks"]["next_call"];
    assert_eq!(
        later_call["arguments"]["max_page_bytes"],
        MIN_GIT_DIFF_HUNKS_PAGE_BYTES
    );
    let exact_record =
        run_parser_ready_worktree_git_diff_hunks_call(&runtime, client_id, repo.path(), later_call)
            .await;
    assert!(!exact_record.success);
    assert_eq!(exact_record.output["reason_code"], "page_record_too_large");
    assert_eq!(exact_record.output["files"], json!([]));
}

#[tokio::test]
async fn git_diff_hunks_fragment_recovery_requires_only_next_bounded_window_to_fit() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    let original = (0..60)
        .map(|line| format!("old-{line:03}-{}\n", "x".repeat(400)))
        .collect::<String>();
    commit_file(repo.path(), "wide-lines.txt", &original, "baseline");
    let changed = (0..60)
        .map(|line| format!("new-{line:03}-{}\n", "y".repeat(400)))
        .collect::<String>();
    fs::write(repo.path().join("wide-lines.txt"), changed).unwrap();

    let runtime = test_runtime();
    let client_id = "diff-hunks-future-byte-ceiling";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    // This test is specifically about a future recovery being blocked by a
    // constrained producer byte page; do not depend on the product default.
    let constrained_page_bytes = 32 * 1024;
    let (page, _, _) = run_runner_git_diff_hunks_page_with_budget(
        &runtime,
        client_id,
        &project,
        repo.path(),
        None,
        10,
        5,
        Some(constrained_page_bytes),
        false,
        None,
    )
    .await;

    assert!(page.success, "{:?}", page.error);
    assert_eq!(page.output["max_page_bytes"], constrained_page_bytes);
    let reasons = page.output["truncation_reasons"].as_array().unwrap();
    assert!(reasons.iter().any(|reason| reason == "hunk_line_limit"));
    assert!(!reasons.iter().any(|reason| reason == "page_byte_budget"));
    let recovery = &page.output["recovery"];
    assert!(recovery["current_hunk"].is_object());
    assert!(recovery["current_hunk"]["next_call"].is_object());
    assert_eq!(
        recovery["current_hunk"]["reason_code"],
        "hunk_fragment_continuation_available"
    );
    assert_git_diff_hunks_sparse_recovery_calls_parse(recovery);
}

#[tokio::test]
async fn git_diff_hunks_line_ceiling_uses_fragment_pagination_without_raising_ceiling() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    let original = (0..250)
        .map(|line| format!("old-{line:03}\n"))
        .collect::<String>();
    commit_file(repo.path(), "too-long.txt", &original, "baseline");
    let changed = (0..250)
        .map(|line| format!("new-{line:03}\n"))
        .collect::<String>();
    fs::write(repo.path().join("too-long.txt"), changed).unwrap();

    let runtime = test_runtime();
    let client_id = "diff-hunks-line-ceiling";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let (bounded, _, _) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        None,
        10,
        5,
        false,
        None,
    )
    .await;
    assert!(bounded.success, "{:?}", bounded.error);
    assert_eq!(bounded.output["has_more"], false);
    assert!(bounded.output["truncation_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason == "hunk_line_limit"));
    let recovery = &bounded.output["recovery"];
    assert!(recovery["current_hunk"].is_object());
    assert!(recovery["current_hunk"]["next_call"].is_object());
    assert_eq!(
        recovery["current_hunk"]["reason_code"],
        "hunk_fragment_continuation_available"
    );
    assert_eq!(
        recovery["current_hunk"]["next_call"]["arguments"]["max_hunk_lines"],
        5
    );

    let (at_ceiling, _, _) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        None,
        10,
        400,
        false,
        None,
    )
    .await;
    assert!(at_ceiling.success, "{:?}", at_ceiling.error);
    let recovery = &at_ceiling.output["recovery"];
    assert!(recovery["current_hunk"]["next_call"].is_object());
    assert_eq!(
        recovery["current_hunk"]["reason_code"],
        "hunk_fragment_continuation_available"
    );
    assert_eq!(
        recovery["current_hunk"]["next_call"]["arguments"]["max_hunk_lines"],
        MAX_MAX_HUNK_LINES
    );
    assert!(
        recovery["current_hunk"]["next_call"]["arguments"]["continuation"]
            .as_str()
            .is_some()
    );
}

#[tokio::test]
async fn git_diff_hunks_fragment_recovery_fails_closed_when_next_complete_line_cannot_fit() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    let huge_old = format!("old-huge-{}", "x".repeat(96 * 1024));
    let original = format!("old-small\n{huge_old}\nold-tail\n");
    commit_file(
        repo.path(),
        "huge-line.txt",
        &original,
        "huge line baseline",
    );
    let huge_new = format!("new-huge-{}", "y".repeat(96 * 1024));
    let changed = format!("new-small\n{huge_new}\nnew-tail\n");
    fs::write(repo.path().join("huge-line.txt"), changed).unwrap();

    let runtime = test_runtime();
    let client_id = "diff-hunks-fragment-byte-impossible";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let (page, _, _) = run_runner_git_diff_hunks_page_with_budget(
        &runtime,
        client_id,
        &project,
        repo.path(),
        Some(vec!["huge-line.txt".to_string()]),
        10,
        2,
        Some(MIN_GIT_DIFF_HUNKS_PAGE_BYTES),
        false,
        None,
    )
    .await;
    assert!(page.success, "{:?}", page.error);
    assert!(page.output["truncation_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason == "hunk_line_limit"));
    let recovery = &page.output["recovery"];
    assert_eq!(
        recovery["current_hunk"]["reason_code"],
        "page_byte_budget_prevents_proven_recovery"
    );
    assert!(recovery["current_hunk"].get("next_call").is_none());
    assert!(recovery.get("later_hunks").is_none());
}

#[tokio::test]
async fn git_diff_hunks_mixed_hunk_fragment_and_later_record_continuations_remain_distinct() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    let original = (0..600)
        .map(|line| format!("old-{line:04}\n"))
        .collect::<String>();
    commit_file(repo.path(), "a-huge.txt", &original, "huge baseline");
    commit_file(repo.path(), "z-later.txt", "before\n", "later baseline");
    let changed = (0..600)
        .map(|line| format!("new-{line:04}\n"))
        .collect::<String>();
    fs::write(repo.path().join("a-huge.txt"), changed).unwrap();
    fs::write(repo.path().join("z-later.txt"), "after\n").unwrap();

    let runtime = test_runtime();
    let client_id = "diff-hunks-mixed-fragment-page";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let (first, _, _) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        None,
        10,
        400,
        false,
        None,
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    assert_eq!(first.output["files"][0]["path"], "a-huge.txt");
    assert_eq!(first.output["has_more"], true);
    let recovery = &first.output["recovery"];
    assert!(recovery["current_hunk"].is_object());
    assert!(recovery["later_hunks"]["next_call"].is_object());
    assert!(recovery["current_hunk"]["next_call"].is_object());
    assert_eq!(
        recovery["current_hunk"]["reason_code"],
        "hunk_fragment_continuation_available"
    );
    let fragment_call = recovery["current_hunk"]["next_call"].clone();
    let later_call = recovery["later_hunks"]["next_call"].clone();
    assert!(first.output.get("next_continuation").is_none());
    let serialized = serde_json::to_string(&first.output).unwrap();
    for call in [&fragment_call, &later_call] {
        let token = call["arguments"]["continuation"].as_str().unwrap();
        assert_eq!(
            serialized.matches(token).count(),
            1,
            "each continuation identity has one projection"
        );
    }

    assert_ne!(
        fragment_call["arguments"]["continuation"],
        later_call["arguments"]["continuation"]
    );
    ToolCall::from_tool_name(
        fragment_call["tool"].as_str().unwrap(),
        fragment_call["arguments"].clone(),
    )
    .expect("fragment call parses");
    ToolCall::from_tool_name(
        later_call["tool"].as_str().unwrap(),
        later_call["arguments"].clone(),
    )
    .expect("later-record call parses");

    let mut current_call = fragment_call;
    let mut fragment_steps = 0usize;
    loop {
        let page = run_parser_ready_worktree_git_diff_hunks_call(
            &runtime,
            client_id,
            repo.path(),
            &current_call,
        )
        .await;
        assert!(page.success, "{:?}", page.error);
        assert_eq!(page.output["files"][0]["path"], "a-huge.txt");
        assert_eq!(page.output["files"][0]["hunks"][0]["continued"], true);
        fragment_steps += 1;
        let Some(next_call) = page
            .output
            .get("recovery")
            .and_then(|value| value.get("current_hunk"))
            .and_then(|value| value.get("next_call"))
            .filter(|value| !value.is_null())
            .cloned()
        else {
            break;
        };
        current_call = next_call;
    }
    assert!(fragment_steps >= 2);

    let later = run_parser_ready_worktree_git_diff_hunks_call(
        &runtime,
        client_id,
        repo.path(),
        &later_call,
    )
    .await;
    assert!(later.success, "{:?}", later.error);
    assert!(later.output["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|file| file["path"] == "z-later.txt"));
    assert!(!later.output["files"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|file| file["hunks"].as_array().into_iter().flatten())
        .any(|hunk| hunk.get("continued").and_then(Value::as_bool) == Some(true)));
}

#[tokio::test]
async fn git_diff_hunks_mixed_byte_omission_keeps_later_hunk_continuation_actionable() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    let huge_old = format!("old-{}\n", "x".repeat(96 * 1024));
    commit_file(repo.path(), "huge.txt", &huge_old, "huge baseline");
    commit_file(repo.path(), "later.txt", "before\n", "later baseline");
    let huge_new = format!("new-{}\n", "y".repeat(96 * 1024));
    fs::write(repo.path().join("huge.txt"), huge_new).unwrap();
    fs::write(repo.path().join("later.txt"), "after\n").unwrap();

    let runtime = test_runtime();
    let client_id = "diff-hunks-mixed-byte-continuation";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let (page, _, _) = run_runner_git_diff_hunks_page(
        &runtime,
        client_id,
        &project,
        repo.path(),
        None,
        10,
        400,
        false,
        None,
    )
    .await;
    assert!(page.success, "{:?}", page.error);
    assert_eq!(page.output["has_more"], true);
    let first_recovery = &page.output["recovery"];
    assert!(first_recovery.get("current_hunk").is_none());
    assert!(first_recovery["later_hunks"]["next_call"].is_object());
    assert_git_diff_hunks_sparse_recovery_calls_parse(first_recovery);

    let huge = run_parser_ready_worktree_git_diff_hunks_call(
        &runtime,
        client_id,
        repo.path(),
        &first_recovery["later_hunks"]["next_call"],
    )
    .await;
    assert!(huge.success, "{:?}", huge.error);
    assert_eq!(huge.output["files"][0]["path"], "huge.txt");
    assert_eq!(huge.output["files"][0]["hunks"][0]["truncated"], true);
    let huge_recovery = &huge.output["recovery"];
    assert_eq!(
        huge_recovery["current_hunk"]["reason_code"],
        "hunk_fragment_continuation_available"
    );
    assert!(huge_recovery["current_hunk"]["next_call"].is_object());
    assert!(huge_recovery["later_hunks"]["next_call"].is_object());
    assert_ne!(
        huge_recovery["current_hunk"]["next_call"]["arguments"]["continuation"],
        huge_recovery["later_hunks"]["next_call"]["arguments"]["continuation"]
    );

    let later = run_parser_ready_worktree_git_diff_hunks_call(
        &runtime,
        client_id,
        repo.path(),
        &huge_recovery["later_hunks"]["next_call"],
    )
    .await;
    assert!(later.success, "{:?}", later.error);
    assert!(later.output["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|file| file["path"] == "later.txt"));
}

#[tokio::test]
async fn git_diff_hunks_malformed_continuation_fails_before_runner_dispatch() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    commit_file(repo.path(), "a.txt", "a0\n", "add a");
    fs::write(repo.path().join("a.txt"), "a1\n").unwrap();
    let runtime = test_runtime();
    let client_id = "diff-hunks-invalid-token";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let result = runtime
        .git_diff_hunks_continued(
            project,
            None,
            Some(1),
            Some(40),
            Some(false),
            Some("not-a-valid-continuation".to_string()),
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.output["reason_code"], "invalid_continuation");
    assert_eq!(result.output["files"], json!([]));
    assert!(result.output["recovery"].get("later_hunks").is_none());
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
}

#[test]
fn git_diff_hunks_parser_handles_modified_empty_and_limits() {
    let binary = "\
diff --git a/binary file.bin b/binary file.bin
index 1111111..2222222 100644
Binary files a/binary file.bin and b/binary file.bin differ
";
    let (binary_files, binary_hunks, binary_truncated) = parse_git_diff_hunks(binary, 10, 20);
    assert!(!binary_truncated);
    assert_eq!(binary_hunks, 0);
    assert_eq!(binary_files.len(), 1);
    assert_eq!(binary_files[0]["path"], "binary file.bin");
    assert_eq!(binary_files[0]["old_path"], "binary file.bin");
    assert_eq!(binary_files[0]["binary"], true);

    let diff = "\
diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,2 +1,3 @@ fn demo()
 line one
-old
+new
+added
";
    let (files, hunk_count, truncated) = parse_git_diff_hunks(diff, 10, 20);
    assert!(!truncated);
    assert_eq!(hunk_count, 1);
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "src/lib.rs");
    assert_eq!(files[0]["status"], "modified");
    assert_eq!(files[0]["hunks"][0]["old_start"], 1);
    assert!(files[0]["hunks"][0]["diff"]
        .as_str()
        .unwrap()
        .contains("+new"));

    let (files, hunk_count, truncated) = parse_git_diff_hunks("", 10, 20);
    assert!(files.is_empty());
    assert_eq!(hunk_count, 0);
    assert!(!truncated);

    let (_files, hunk_count, truncated) = parse_git_diff_hunks(diff, 0, 20);
    assert_eq!(hunk_count, 0);
    assert!(truncated);

    let (files, _hunk_count, truncated) = parse_git_diff_hunks(diff, 10, 2);
    assert!(truncated);
    assert_eq!(files[0]["hunks"][0]["truncated"], true);
}

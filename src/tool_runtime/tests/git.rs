//! Git tests for tool_runtime.

use super::super::git::*;
use super::super::helpers::*;
use super::super::*;
use super::support::*;
use crate::shell_protocol::{ShellAgentResultRequest, ShellClientCapabilities};
use crate::tool_runtime::ToolRuntime;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use webcodex_workspace::file_read_range::MAX_SERIALIZED_OUTPUT_BYTES;

async fn register_structured_git_agent_at_path(
    runtime: &ToolRuntime,
    client_id: &str,
    project_id: &str,
    root: &Path,
) -> String {
    let project_path = root.to_string_lossy().to_string();
    register_agent_with_projects(
        runtime,
        client_id,
        None,
        ShellClientCapabilities {
            shell: true,
            git: true,
            structured_process_argv: true,
            ..Default::default()
        },
        vec![registered_project(project_id, &project_path)],
    )
    .await;
    crate::tool_runtime::agent_project_runtime_id(client_id, project_id)
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

    let request = next_patch_agent_request(&runtime, "restore-target-substring")
        .await
        .expect("git_restore_paths should enqueue an agent process request");
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
    let request = next_patch_agent_request(&runtime, "literal-git-paths")
        .await
        .expect("git restore should enqueue typed argv");
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
    let request = next_patch_agent_request(&runtime, "literal-git-paths")
        .await
        .expect("git clean should enqueue typed argv");
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
async fn git_path_mutation_capability_preflight_matches_structured_process_runtime() {
    let runtime = runtime_with_agent_project("restore-structured-only");
    register_agent(
        &runtime,
        "restore-structured-only",
        None,
        ShellClientCapabilities {
            shell: false,
            structured_process_argv: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("restore-structured-only");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::GitRestorePaths {
                        project,
                        paths: vec!["safe.txt".to_string()],
                        session_id: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });

    let request = next_patch_agent_request(&runtime, "restore-structured-only")
        .await
        .expect("structured-process-only Runner should reach typed process dispatch");
    assert_eq!(request.kind, "run_process");
    assert!(request.command.is_empty());
    let process = request.process.as_ref().expect("typed git restore process");
    assert_eq!(process.executable, "git");
    assert_eq!(
        process.args,
        ["restore", "--", "safe.txt"].map(str::to_string)
    );
    complete_patch_agent_request(
        &runtime,
        "restore-structured-only",
        &request.request_id,
        0,
        "",
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);

    let runtime = runtime_with_agent_project("discard-shell-only");
    register_agent(
        &runtime,
        "discard-shell-only",
        None,
        ShellClientCapabilities {
            shell: true,
            git: true,
            structured_process_argv: false,
            ..Default::default()
        },
    )
    .await;
    let bootstrap = auth_context(None, true);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::DiscardUntracked {
                project: agent_test_project_id("discard-shell-only"),
                paths: vec!["safe.txt".to_string()],
                session_id: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "not_started");
    assert_eq!(result.output["command_started"], false);
    assert_eq!(result.output["failure_kind"], "capability_unavailable");
    let error = result.error.as_deref().unwrap_or_default();
    assert!(error.contains("structured_process_argv"), "{error}");
    assert!(error.contains("no shell fallback"), "{error}");
    assert!(
        next_patch_agent_request(&runtime, "discard-shell-only")
            .await
            .is_none(),
        "capability preflight must reject before enqueue"
    );
}

#[tokio::test]
async fn git_restore_missing_structured_process_capability_is_definite_not_started() {
    let runtime = runtime_with_agent_project("restore-no-argv");
    register_agent(
        &runtime,
        "restore-no-argv",
        None,
        ShellClientCapabilities {
            shell: true,
            git: true,
            structured_process_argv: false,
            ..Default::default()
        },
    )
    .await;

    let result = runtime
        .git_restore_paths(
            agent_test_project_id("restore-no-argv"),
            vec!["safe.txt".to_string()],
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "not_started");
    assert_eq!(result.output["failure_kind"], "capability_unavailable");
    assert!(next_patch_agent_request(&runtime, "restore-no-argv")
        .await
        .is_none());
}

#[tokio::test]
async fn git_restore_stays_sync_on_structured_job_capable_runner() {
    let runtime = runtime_with_agent_project("restore-sync-job-capable");
    register_agent(
        &runtime,
        "restore-sync-job-capable",
        None,
        ShellClientCapabilities {
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

    let request = next_patch_agent_request(&runtime, "restore-sync-job-capable")
        .await
        .expect("git restore should stay on the synchronous process path");
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
        next_patch_agent_request(&runtime, "restore-sync-job-capable")
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
        ShellClientCapabilities {
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
    let request = next_patch_agent_request(&runtime, "restore-uncertain")
        .await
        .expect("git restore should dispatch once");
    assert_eq!(request.kind, "run_process");

    runtime
        .shell_clients
        .set_last_seen_for_test("restore-uncertain", chrono::Utc::now().timestamp() - 120)
        .await;
    register_agent_with_instance(
        &runtime,
        "restore-uncertain",
        "inst-b",
        None,
        ShellClientCapabilities {
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
    let retry = next_agent_request_for_instance(&runtime, "restore-uncertain", "inst-b").await;
    assert!(
        retry.is_none(),
        "uncertain mutation must not be retried: {retry:?}"
    );
}

#[test]
fn git_diff_hunks_tool_is_known_and_schema_is_bounded() {
    assert!(is_known_tool_name("git_diff_hunks"));
    let call = ToolCall::from_tool_name(
        "git_diff_hunks",
        json!({
            "project":"agent:oe:webcodex",
            "paths":["src/runtime_http.rs"],
            "max_hunks":20,
            "max_hunk_lines":120,
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
            continuation: Some(continuation),
            ..
        } if project == "agent:oe:webcodex" && continuation == "opaque-continuation"
    ));

    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "git_diff_hunks");
    let props = spec.input_schema["properties"].as_object().unwrap();
    for field in [
        "project",
        "paths",
        "max_hunks",
        "max_hunk_lines",
        "cached",
        "continuation",
    ] {
        assert!(props.contains_key(field), "missing {}", field);
    }
    assert_eq!(
        props["continuation"]["maxLength"],
        GIT_DIFF_HUNKS_CONTINUATION_MAX_BYTES
    );
    assert!(ToolCall::from_tool_name(
        "git_diff_hunks",
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
        "hunk_count",
        "truncated",
        "truncation_reasons",
        "has_more",
        "next_continuation",
        "exit_code",
        "stderr",
    ] {
        assert!(output_props.contains_key(field), "missing {}", field);
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
        "cached": true,
        "continuation": continuation,
    });

    let raw_summary = super::super::tool_audit::session_log_arguments_for_tool_request(
        "git_diff_hunks",
        &arguments,
    );
    assert_eq!(raw_summary["project"], "agent:oe:webcodex");
    assert_eq!(
        raw_summary["paths"],
        json!(["src/runtime_http.rs", "src/tool_runtime/git.rs"])
    );
    assert_eq!(raw_summary["max_hunks"], 7);
    assert_eq!(raw_summary["max_hunk_lines"], 33);
    assert_eq!(raw_summary["cached"], true);
    assert_eq!(raw_summary["continuation_present"], true);
    assert!(raw_summary.get("continuation").is_none());
    assert!(!serde_json::to_string(&raw_summary)
        .unwrap()
        .contains(continuation));

    let call = ToolCall::from_tool_name("git_diff_hunks", arguments.clone()).unwrap();
    let typed_summary = call.session_log_arguments();
    assert_eq!(typed_summary["project"], "agent:oe:webcodex");
    assert_eq!(typed_summary["paths"], raw_summary["paths"]);
    assert_eq!(typed_summary["max_hunks"], 7);
    assert_eq!(typed_summary["max_hunk_lines"], 33);
    assert_eq!(typed_summary["cached"], true);
    assert!(typed_summary.get("continuation").is_none());
    assert!(!serde_json::to_string(&typed_summary)
        .unwrap()
        .contains(continuation));

    let defensive =
        super::super::sessions::session_input_summary_for_tool("git_diff_hunks", &arguments);
    assert_eq!(defensive["project"], "agent:oe:webcodex");
    assert_eq!(defensive["paths"], raw_summary["paths"]);
    assert_eq!(defensive["max_hunks"], 7);
    assert_eq!(defensive["max_hunk_lines"], 33);
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
        "git_diff_hunks",
        &arguments,
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
    assert_eq!(input_summary["cached"], true);
    assert!(input_summary.get("continuation").is_none());
    assert!(!serde_json::to_string(input_summary)
        .unwrap()
        .contains(continuation));
}

#[test]
fn show_changes_tool_is_known_and_parses() {
    assert!(is_known_tool_name("show_changes"));
    let call = ToolCall::from_tool_name(
        "show_changes",
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
    let spec = spec_named(&specs, "show_changes");
    let output_props = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert!(
        output_props.contains_key("verdict"),
        "show_changes output schema should expose verdict"
    );
    assert!(
        output_props.contains_key("diff_stat_status"),
        "show_changes output schema should expose strict diff-stat observation"
    );
}

async fn run_agent_git_diff_hunks_page(
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
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.to_string();
        async move {
            runtime
                .git_diff_hunks_continued(
                    project,
                    paths,
                    Some(max_hunks),
                    Some(max_hunk_lines),
                    Some(cached),
                    continuation,
                )
                .await
        }
    });
    let request = next_patch_agent_request(runtime, client_id)
        .await
        .expect("git_diff_hunks should enqueue one bounded agent request");
    assert_eq!(request.kind, "run_internal_posix_script");
    assert_eq!(
        request.cwd.as_deref(),
        Some(repo.to_string_lossy().as_ref())
    );
    assert!(request.command.is_empty());
    let script = request
        .script
        .as_ref()
        .expect("git_diff_hunks must carry a typed internal script")
        .script
        .clone();
    let (exit_code, stdout, stderr) = run_agent_shell_request_locally(&request);
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

fn git_test_command_ok(repo: &Path, command: &str) {
    let (exit_code, stdout, stderr, _) = run_command_sync(command, repo, 30);
    assert_eq!(
        exit_code, 0,
        "{command} failed: stdout={stdout} stderr={stderr}"
    );
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
    let project = register_agent_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let mut continuation = None;
    let mut logical_files = Vec::new();
    let mut logical_hunks = Vec::new();
    let mut seen_tokens = HashSet::new();
    let mut finished = false;

    for _ in 0..10 {
        let prior_token = continuation.clone();
        let (result, _stdout_bytes, command) = run_agent_git_diff_hunks_page(
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
            assert_eq!(result.output["next_continuation"], Value::Null);
            finished = true;
            break;
        }
        let next = result.output["next_continuation"]
            .as_str()
            .expect("non-final page continuation")
            .to_string();
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
async fn git_diff_hunks_large_raw_diff_is_bounded_before_agent_transport_capture() {
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
        raw_diff.len() > MAX_SERIALIZED_OUTPUT_BYTES,
        "fixture did not exceed transport-sized retention: {} bytes",
        raw_diff.len()
    );

    let runtime = test_runtime();
    let client_id = "diff-hunks-large";
    let project = register_agent_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let (result, agent_stdout_bytes, _command) = run_agent_git_diff_hunks_page(
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
    assert!(
        agent_stdout_bytes <= GIT_DIFF_HUNKS_PAGE_BYTES + 4096,
        "producer sent {agent_stdout_bytes} bytes through ordinary transport"
    );
    assert_eq!(result.output["files"][0]["path"], "large-0.txt");
    assert_eq!(result.output["has_more"], true);
    assert!(result.output["next_continuation"].as_str().is_some());
    assert!(
        serde_json::to_vec(&result).unwrap().len() <= MAX_SERIALIZED_OUTPUT_BYTES,
        "serialized result exceeded model envelope"
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
    let project = register_agent_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let (page, _, _) = run_agent_git_diff_hunks_page(
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
    let token = page.output["next_continuation"]
        .as_str()
        .expect("first page continuation")
        .to_string();
    fs::write(repo.path().join("b.txt"), "b2\nextra\n").unwrap();

    let (stale, _, _) = run_agent_git_diff_hunks_page(
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
    assert_eq!(stale.output["next_continuation"], Value::Null);
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
    let project = register_agent_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let (page, _, _) = run_agent_git_diff_hunks_page(
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
    let token = page.output["next_continuation"]
        .as_str()
        .expect("cached continuation")
        .to_string();
    fs::write(repo.path().join("b.txt"), "b2\n").unwrap();
    git_test_command_ok(repo.path(), "git add -- b.txt");

    let (stale, _, _) = run_agent_git_diff_hunks_page(
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
    assert_eq!(stale.output["next_continuation"], Value::Null);
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
    let project = register_agent_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let (page, _, _) = run_agent_git_diff_hunks_page(
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
    let token = page.output["next_continuation"]
        .as_str()
        .expect("scoped continuation")
        .to_string();
    fs::write(repo.path().join("outside.txt"), "outside1\n").unwrap();

    let (continued, _, command) = run_agent_git_diff_hunks_page(
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
        register_agent_project_at_path(&runtime, other_client_id, "repo", repo.path()).await;
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
    assert!(next_patch_agent_request(&runtime, other_client_id)
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
    assert!(next_patch_agent_request(&runtime, client_id)
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
    assert!(next_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
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
    git_test_command_ok(repo.path(), "git add -- . && git commit -m binary-baseline");
    for (index, name) in names.iter().enumerate() {
        let mut bytes = vec![0u8; 512];
        bytes[1] = index as u8;
        bytes[2] = 1;
        fs::write(repo.path().join(name), bytes).unwrap();
    }

    let runtime = test_runtime();
    let client_id = "diff-hunks-binary";
    let project = register_agent_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let mut continuation = None;
    let mut returned = Vec::new();
    let mut finished = false;
    for _ in 0..10 {
        let (page, agent_stdout_bytes, _) = run_agent_git_diff_hunks_page(
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
        assert!(agent_stdout_bytes <= GIT_DIFF_HUNKS_PAGE_BYTES + 4096);
        for file in page.output["files"].as_array().unwrap() {
            assert_ne!(file.get("continued").and_then(Value::as_bool), Some(true));
            returned.push(file["path"].as_str().unwrap().to_string());
        }
        if page.output["has_more"] == false {
            assert_eq!(page.output["next_continuation"], Value::Null);
            finished = true;
            break;
        }
        assert!(page.output["truncation_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason == "page_byte_budget"));
        continuation = Some(
            page.output["next_continuation"]
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
    let original = (0..300)
        .map(|line| format!("old-{line:03}\n"))
        .collect::<String>();
    commit_file(repo.path(), "long.txt", &original, "add long");
    let changed = (0..300)
        .map(|line| format!("new-{line:03}\n"))
        .collect::<String>();
    fs::write(repo.path().join("long.txt"), changed).unwrap();

    let runtime = test_runtime();
    let client_id = "diff-hunks-line-limit";
    let project = register_agent_project_at_path(&runtime, client_id, "repo", repo.path()).await;
    let (page, _, _) = run_agent_git_diff_hunks_page(
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
    assert_eq!(page.output["next_continuation"], Value::Null);
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
}

#[tokio::test]
async fn git_diff_hunks_malformed_continuation_fails_before_runner_dispatch() {
    let repo = tempfile::tempdir().unwrap();
    init_git_repo(repo.path());
    commit_file(repo.path(), "a.txt", "a0\n", "add a");
    fs::write(repo.path().join("a.txt"), "a1\n").unwrap();
    let runtime = test_runtime();
    let client_id = "diff-hunks-invalid-token";
    let project = register_agent_project_at_path(&runtime, client_id, "repo", repo.path()).await;
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
    assert_eq!(result.output["next_continuation"], Value::Null);
    assert!(next_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
}

#[test]
fn git_diff_hunks_parser_handles_modified_empty_and_limits() {
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

#[test]
fn show_changes_command_is_read_only() {
    let without_diff = show_changes_command(false, 20, 80);
    let with_diff = show_changes_command(true, 20, 80);
    assert!(
        without_diff.len() <= crate::shell_protocol::RAW_SHELL_COMMAND_MAX_BYTES,
        "include_diff=false command is {} bytes",
        without_diff.len()
    );
    assert!(
        with_diff.len() <= crate::shell_protocol::RAW_SHELL_COMMAND_MAX_BYTES,
        "include_diff=true command is {} bytes",
        with_diff.len()
    );
    eprintln!(
        "show_changes_command_lengths without_diff={} with_diff={}",
        without_diff.len(),
        with_diff.len()
    );
    for cmd in [&without_diff, &with_diff] {
        assert!(cmd.contains("git status --porcelain=v1 -b"));
        assert!(cmd.contains("git log -1"));
        assert!(cmd.contains("git diff --stat"));
        assert!(cmd.contains("LC_ALL=C; export LC_ALL"));
        assert!(!cmd.contains("head_buf=$("), "HEAD must be streamed: {cmd}");
        assert!(
            !cmd.contains("stat_buf=$("),
            "diff-stat must be streamed: {cmd}"
        );
        assert!(
            !cmd.contains("while IFS= read -r hline"),
            "HEAD subject must not be read into an unbounded shell variable: {cmd}"
        );
        assert!(
            !cmd.contains("head_buf="),
            "HEAD must not accumulate an unbounded multi-line buffer: {cmd}"
        );
        assert!(
            cmd.contains("head_subject=$(git log -1 --format=%s \"$head_commit\" 2>/dev/null | dd bs=1 count=$((head_subject_limit+1)) 2>/dev/null)"),
            "HEAD subject producer must be byte-bounded before command substitution: {cmd}"
        );
        assert!(
            cmd.contains(&format!(
                "head_subject_limit=$(({}-head_prefix_bytes))",
                SHOW_CHANGES_HEAD_BYTES
            )),
            "HEAD subject limit must derive from the remaining frame budget: {cmd}"
        );
        let forbidden = ["python3", "-c"].join(" ");
        assert!(
            !cmd.contains(&forbidden),
            "show_changes command must not invoke a Python helper: {cmd}"
        );
        for forbidden in [
            " clean",
            " restore",
            " add",
            " commit",
            " reset",
            " checkout",
            " push",
            " stash",
            " merge",
            " rebase",
            " rm ",
        ] {
            assert!(
                !cmd.contains(forbidden),
                "show_changes command must not contain '{}': {}",
                forbidden,
                cmd
            );
        }
    }
}

#[test]
fn show_changes_command_emits_bounded_metadata_frames() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    for i in 0..3 {
        std::fs::write(tmp.path().join(format!("new{i}.txt")), "x\n").unwrap();
    }
    std::fs::write(tmp.path().join("README.md"), "hello\nchanged\n").unwrap();
    let cmd = show_changes_command(true, 2, 5);
    let (exit, stdout, stderr, _) = run_command_sync(&cmd, tmp.path(), 30);
    assert_eq!(
        exit, 0,
        "command failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("## "), "missing branch header: {stdout}");
    assert!(
        stdout.contains("files_total="),
        "missing files_total: {stdout}"
    );
    assert!(
        stdout.contains("files_returned="),
        "missing files_returned: {stdout}"
    );
    assert!(
        stdout.contains("files_limit="),
        "missing files_limit: {stdout}"
    );
    assert!(
        stdout.contains("diff_hunks_returned="),
        "missing diff meta: {stdout}"
    );
    assert!(
        stdout.contains("diff_hunks_truncated="),
        "missing diff truncation: {stdout}"
    );
    for field in [
        "status_bytes=",
        "head_bytes=",
        "diff_stat_bytes=",
        "diff_trunc_hunk_count=",
        "diff_trunc_hunk_lines=",
        "diff_trunc_bytes=",
        "diff_bytes=",
    ] {
        assert!(stdout.contains(field), "missing {field}: {stdout}");
    }
    // No raw placeholder may leak into the generated command.
    assert!(
        !cmd.contains("__SENTINEL__"),
        "sentinel placeholder leaked: {cmd}"
    );
    assert!(
        !cmd.contains("__HUNK_LIMIT__") && !cmd.contains("__LINE_LIMIT__"),
        "limit placeholder leaked: {cmd}"
    );
}

#[test]
fn show_changes_bounds_status_files_and_keeps_totals_exact() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    // Exceed the production status-file cap by many untracked files.
    let cap = 200usize;
    let total = cap + 50;
    for i in 0..total {
        std::fs::write(tmp.path().join(format!("u{i}.txt")), "x\n").unwrap();
    }
    let output = bounded_show_changes_output(tmp.path(), true, 4, 80);
    assert_eq!(
        output["files_total"].as_u64(),
        Some(total as u64),
        "files_total must count every entry: {output}"
    );
    assert_eq!(
        output["files_returned"].as_u64(),
        Some(cap as u64),
        "files_returned must hit the cap: {output}"
    );
    assert_eq!(
        output["files_truncated"], true,
        "must report truncation: {output}"
    );
    assert_eq!(
        output["files_limit"].as_u64(),
        Some(cap as u64),
        "files_limit must equal the cap: {output}"
    );
    let files = output["files"].as_array().unwrap();
    assert_eq!(
        files.len(),
        cap,
        "files array length must equal returned: {output}"
    );
    // Per-category counts must reflect ALL entries, not the truncated subset.
    assert_eq!(
        output["counts"]["untracked"].as_u64(),
        Some(total as u64),
        "untracked count must be exact over all entries: {output}"
    );
    assert_eq!(output["clean"], false, "truncated dirty repo is not clean");
    // The production-side count cap fired, so output_truncated is true with a
    // stable reason; the bound is still provably transport-safe.
    assert_eq!(output["transport_safe"], true);
    assert_eq!(output["output_truncated"], true);
    let reasons = output["truncation_reasons"]
        .as_array()
        .expect("truncation reasons array");
    assert!(
        reasons
            .iter()
            .any(|r| r.as_str() == Some("status_file_count_limit")),
        "expected status_file_count_limit reason: {reasons:?}"
    );
    assert_show_changes_envelope_value_matches_schema(&output, "bounded status");
}

/// Build a long (but <= NAME_MAX) leaf filename so a *real* >256 KiB `git
/// status` can be produced from many tracked files in a single flat directory,
/// without relying on a single path exceeding filesystem limits. Each name is
/// ~237 printable chars plus a per-file suffix and `.x`, staying under the
/// 255-byte NAME_MAX.
fn long_leaf_name(i: usize) -> String {
    format!("{}{i}.x", "g".repeat(234))
}

#[test]
fn show_changes_status_output_over_256k_keeps_branch_header_observable() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    // A *real* >256 KiB status: ~1200 tracked files with long (~237-byte)
    // names in a single flat directory, committed and then modified. The full
    // status legitimately exceeds the transport cap, not just the count limit.
    let dir = tmp.path().join("d");
    std::fs::create_dir_all(&dir).unwrap();
    let total = 1200usize;
    for i in 0..total {
        let path = dir.join(long_leaf_name(i));
        std::fs::write(&path, "x\n").unwrap();
    }
    let (add_exit, _, add_stderr, _) = run_command_sync("git add -A", tmp.path(), 30);
    assert_eq!(add_exit, 0, "git add failed: {add_stderr}");
    // Use `-q` so the commit summary (which can be very large for many files)
    // does not flood stdout under the synchronous command runner.
    let (commit_exit, _, commit_stderr, _) = run_command_sync("git commit -qm add", tmp.path(), 30);
    assert_eq!(commit_exit, 0, "git commit failed: {commit_stderr}");
    for i in 0..total {
        let path = dir.join(long_leaf_name(i));
        std::fs::write(&path, "y\nx\n").unwrap();
    }
    // First assert the *unbounded* raw status actually exceeds 256 KiB. The raw
    // status legitimately exceeds the OS pipe buffer, so use the full-capture
    // helper that drains the pipes concurrently (the polling helper deadlocks).
    let (raw_exit, raw_stdout, raw_stderr) =
        run_command_full_capture("git status --porcelain=v1 -b", tmp.path(), 30);
    assert_eq!(
        raw_exit, 0,
        "raw status failed\nstdout:\n{raw_stdout}\nstderr:\n{raw_stderr}"
    );
    let raw_status_bytes = raw_stdout.len();
    assert!(
        raw_status_bytes > 256 * 1024,
        "raw status must exceed 256 KiB to cover the transport cap; got {raw_status_bytes} bytes"
    );
    // The bounded show_changes command must stay within the production budget.
    // Its output is also larger than the pipe buffer, so use full capture.
    let cmd = show_changes_command(false, 20, 80);
    let (exit, stdout, stderr) = run_command_full_capture(&cmd, tmp.path(), 30);
    assert_eq!(
        exit, 0,
        "command failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let bounded_stdout_bytes = stdout.len();
    assert!(
        bounded_stdout_bytes <= SHOW_CHANGES_OUTPUT_BUDGET_BYTES,
        "bounded stdout must stay within the production budget; got {bounded_stdout_bytes} bytes (budget {})",
        SHOW_CHANGES_OUTPUT_BUDGET_BYTES
    );
    // Simulate the Runner/Shell transport's 256 KiB tail retention: since the
    // bounded output already fits the budget (< 256 KiB), the tail keeps the
    // whole stream verbatim with no truncation marker.
    let transported = simulate_transport_tail(&stdout, 256 * 1024);
    assert_eq!(
        transported, stdout,
        "bounded output must be unchanged by the transport tail"
    );
    let frames = split_show_changes_stdout(&transported, false);
    assert!(
        frames
            .status
            .lines()
            .any(|line| parse_status_header(line).is_some()),
        "branch header must survive transport tail retention: {transported}"
    );
    let observation =
        parse_show_changes_status_observation(&frames.status, &frames.status_result, "");
    assert!(observation.status_observed());
    // Totals stay exact even though records were bounded.
    assert_eq!(frames.files_total, Some(total));
    assert_eq!(frames.files_truncated, Some(true));
    assert!(frames.files_returned.unwrap_or(0) <= SHOW_CHANGES_MAX_STATUS_FILES);
    // Parse the structured output to assert transport/truncation fields.
    let output = parse_show_changes_output_with_observation(
        "demo",
        &frames.status,
        &frames.head,
        &frames.stat,
        None,
        20,
        80,
        Some(exit),
        &stderr,
        observation,
        &frames,
    );
    assert_eq!(output["transport_safe"], true);
    assert_eq!(output["output_truncated"], true);
    let reasons = output["truncation_reasons"]
        .as_array()
        .expect("truncation reasons array");
    assert!(
        reasons
            .iter()
            .any(|r| r.as_str() == Some("status_file_count_limit")
                || r.as_str() == Some("status_byte_budget")),
        "expected a status count or byte budget reason: {reasons:?}"
    );
    assert_show_changes_envelope_value_matches_schema(&output, "256k status");
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[test]
fn show_changes_head_fields_match_git_in_sh_and_bash() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let subject = "fix: preserve labelled HEAD fields exactly";
    commit_file(tmp.path(), "README.md", "hello\n", subject);

    let expected = |git_command: &str| {
        let (exit, stdout, stderr) = run_command_full_capture(git_command, tmp.path(), 30);
        assert_eq!(exit, 0, "{git_command} failed: {stderr}");
        stdout.trim_end_matches('\n').to_string()
    };
    let expected_commit = expected("git rev-parse HEAD");
    let expected_short = expected("git rev-parse --short HEAD");
    let expected_summary = expected("git log -1 --format=%s");
    let command = show_changes_command(false, 20, 80);

    for shell in ["sh", "bash"] {
        let wrapped = format!("{shell} -c {}", shell_single_quote(&command));
        let (exit, stdout, stderr) = run_command_full_capture(&wrapped, tmp.path(), 30);
        assert_eq!(exit, 0, "{shell} failed: {stderr}\n{stdout}");
        let frames = split_show_changes_stdout(&stdout, false);
        assert_eq!(frames.head_bytes, Some(frames.head.len()));
        let output =
            bounded_show_changes_output_from_frames(&frames, tmp.path(), false, 20, 80, &stderr);
        assert_eq!(
            output["head"]["commit"].as_str(),
            Some(expected_commit.as_str()),
            "{shell} commit"
        );
        assert_eq!(
            output["head"]["short"].as_str(),
            Some(expected_short.as_str()),
            "{shell} short"
        );
        assert_eq!(
            output["head"]["summary"].as_str(),
            Some(expected_summary.as_str()),
            "{shell} summary"
        );
        assert_eq!(output["transport_safe"], true, "{shell}: {output}");
        eprintln!(
            "head_shell={shell} commit={expected_commit} short={expected_short} summary={expected_summary}"
        );
    }
}

#[test]
fn show_changes_unborn_repository_emits_empty_complete_head_frame() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let command = show_changes_command(false, 20, 80);
    let wrapped = format!("sh -c {}", shell_single_quote(&command));
    let (exit, stdout, stderr) = run_command_full_capture(&wrapped, tmp.path(), 30);
    assert_eq!(exit, 0, "unborn show_changes failed: {stderr}\n{stdout}");
    assert!(stdout.len() <= SHOW_CHANGES_OUTPUT_BUDGET_BYTES);
    let frames = split_show_changes_stdout(&stdout, false);
    assert!(frames.head.is_empty());
    assert!(frames.head_exit.is_some_and(|value| value != 0));
    assert_eq!(frames.head_truncated, Some(false));
    assert_eq!(frames.head_bytes, Some(0));
    let output =
        bounded_show_changes_output_from_frames(&frames, tmp.path(), false, 20, 80, &stderr);
    assert_eq!(output["head"]["commit"], Value::Null);
    assert_eq!(output["head"]["short"], Value::Null);
    assert_eq!(output["head"]["summary"], Value::Null);
    assert_eq!(output["transport_safe"], true);
}

#[test]
fn show_changes_crlf_diff_bytes_match_parsed_frame_exactly() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "crlf.txt", "before\r\n", "track CRLF");
    std::fs::write(tmp.path().join("crlf.txt"), b"before\r\nafter\r\n").unwrap();

    let (_, stdout, stderr) = run_bounded_show_changes_full(tmp.path(), true, 20, 80);
    let frames = split_show_changes_stdout(&stdout, true);
    assert!(
        frames.diff.ends_with('\r'),
        "final CR from CRLF diff line was lost: {:?}",
        frames.diff.as_bytes().last()
    );
    assert_eq!(frames.diff_bytes, Some(frames.diff.len()));
    let output =
        bounded_show_changes_output_from_frames(&frames, tmp.path(), true, 20, 80, &stderr);
    assert_eq!(output["transport_safe"], true, "{output}");
    eprintln!(
        "crlf_diff frame_bytes={} metadata_bytes={}",
        frames.diff.len(),
        frames.diff_bytes.unwrap()
    );
}

fn multibyte_status_path(i: usize) -> std::path::PathBuf {
    let mut path = std::path::PathBuf::new();
    for level in 0..3 {
        path.push(format!("{}-{level}", "界".repeat(60)));
    }
    path.push(format!("文件-{i:03}.txt"));
    path
}

#[test]
fn show_changes_bash_utf8_locale_counts_multibyte_status_in_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let total = 500usize;
    for i in 0..total {
        let relative = multibyte_status_path(i);
        let path = tmp.path().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "before\n").unwrap();
    }
    let (setup_exit, _, setup_stderr) = run_command_full_capture(
        "git config core.quotePath false && git add -A && git commit -qm multibyte",
        tmp.path(),
        60,
    );
    assert_eq!(setup_exit, 0, "setup failed: {setup_stderr}");
    for i in 0..total {
        std::fs::write(tmp.path().join(multibyte_status_path(i)), "after\n").unwrap();
    }
    let (raw_exit, raw_status, raw_stderr) = run_command_full_capture(
        "LC_ALL=zh_CN.utf8 git status --porcelain=v1 -b",
        tmp.path(),
        60,
    );
    assert_eq!(raw_exit, 0, "raw status failed: {raw_stderr}");
    assert!(
        raw_status.len() > 256 * 1024,
        "raw status bytes={}",
        raw_status.len()
    );

    let cmd = show_changes_command(false, 20, 80);
    let bash_cmd = format!("LC_ALL=zh_CN.utf8 bash -c {}", shell_single_quote(&cmd));
    let (exit, stdout, stderr) = run_command_full_capture(&bash_cmd, tmp.path(), 60);
    assert_eq!(exit, 0, "bash command failed: {stderr}");
    assert!(
        stdout.len() <= SHOW_CHANGES_OUTPUT_BUDGET_BYTES,
        "bounded bytes={}",
        stdout.len()
    );
    assert_eq!(
        simulate_transport_tail(&stdout, 256 * 1024).as_bytes(),
        stdout.as_bytes()
    );

    let frames = split_show_changes_stdout(&stdout, false);
    let observation =
        parse_show_changes_status_observation(&frames.status, &frames.status_result, &stderr);
    assert!(observation.status_observed());
    assert!(frames
        .status
        .lines()
        .any(|line| parse_status_header(line).is_some()));
    assert_eq!(frames.files_total, Some(total));
    assert_eq!(frames.counts_modified, Some(total));
    let output =
        bounded_show_changes_output_from_frames(&frames, tmp.path(), false, 20, 80, &stderr);
    assert_eq!(output["transport_safe"], true);
    eprintln!(
        "multibyte_status raw_bytes={} bounded_bytes={}",
        raw_status.len(),
        stdout.len()
    );
}

#[test]
fn show_changes_transport_safe_requires_every_modern_metadata_frame() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "before\n", "initial");
    std::fs::write(tmp.path().join("README.md"), "after\n").unwrap();
    let (_, stdout, stderr) = run_bounded_show_changes_full(tmp.path(), true, 20, 80);
    let frames = split_show_changes_stdout(&stdout, true);
    let complete =
        bounded_show_changes_output_from_frames(&frames, tmp.path(), true, 20, 80, &stderr);
    assert_eq!(complete["transport_safe"], true);

    let mut variants = Vec::new();
    let mut missing_status = frames.clone();
    missing_status.status_bytes = None;
    variants.push(("status", missing_status));
    let mut missing_head = frames.clone();
    missing_head.head_bytes = None;
    variants.push(("head", missing_head));
    let mut missing_stat = frames.clone();
    missing_stat.diff_stat_bytes = None;
    variants.push(("stat", missing_stat));
    let mut missing_diff = frames.clone();
    missing_diff.diff_trunc_bytes = None;
    variants.push(("diff", missing_diff));

    for (label, incomplete) in variants {
        let output =
            bounded_show_changes_output_from_frames(&incomplete, tmp.path(), true, 20, 80, &stderr);
        assert_eq!(output["transport_safe"], false, "missing {label} metadata");
    }

    let mut wrong_diff_bytes = frames.clone();
    wrong_diff_bytes.diff_bytes = Some(frames.diff.len() + 1);
    let wrong_diff_output = bounded_show_changes_output_from_frames(
        &wrong_diff_bytes,
        tmp.path(),
        true,
        20,
        80,
        &stderr,
    );
    assert_eq!(wrong_diff_output["transport_safe"], false);

    let mut oversized_stat = frames.clone();
    oversized_stat.stat = "x".repeat(SHOW_CHANGES_DIFF_STAT_BYTES + 1);
    oversized_stat.diff_stat_bytes = Some(oversized_stat.stat.len());
    let oversized_output =
        bounded_show_changes_output_from_frames(&oversized_stat, tmp.path(), true, 20, 80, &stderr);
    assert_eq!(oversized_output["transport_safe"], false);
    eprintln!(
        "invalid_metadata wrong_diff_bytes_transport_safe={} oversized_stat_transport_safe={}",
        wrong_diff_output["transport_safe"], oversized_output["transport_safe"]
    );
}

#[test]
fn show_changes_status_config_error_is_not_reported_clean() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    let (config_exit, _, config_stderr, _) = run_command_sync(
        "git config status.showUntrackedFiles invalid",
        tmp.path(),
        30,
    );
    assert_eq!(
        config_exit, 0,
        "failed to set regression config: {config_stderr}"
    );
    let cmd = show_changes_command(false, 20, 80);
    let (exit, stdout, _stderr, _) = run_command_sync(&cmd, tmp.path(), 30);
    assert_ne!(exit, 0, "status config error must not exit 0");
    let frames = split_show_changes_stdout(&stdout, false);
    let observation =
        parse_show_changes_status_observation(&frames.status, &frames.status_result, &_stderr);
    assert_eq!(observation.as_json()["status"], "command_failed");
    assert_eq!(
        observation.as_json()["reason_code"],
        "git_status_config_error"
    );
    // A failed status must never claim cleanliness.
    let output = parse_show_changes_output(
        "demo",
        &frames.status,
        &frames.head,
        &frames.stat,
        None,
        20,
        80,
        Some(exit),
        &_stderr,
    );
    assert_eq!(output["clean"], Value::Null);
    assert_eq!(output["counts"]["conflicted"], Value::Null);
}

#[test]
fn show_changes_include_diff_false_omits_diff_and_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    std::fs::write(tmp.path().join("README.md"), "hello\nchanged\n").unwrap();
    let output = bounded_show_changes_output(tmp.path(), false, 20, 80);
    assert!(
        output.get("hunks").is_none(),
        "no hunks without include_diff"
    );
    assert!(output.get("hunk_count").is_none());
    assert!(output.get("hunks_truncated").is_none());
}

#[test]
fn show_changes_diff_respects_max_hunks() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "a.txt", "a\n", "initial");
    // Three modified files -> three diff hunks; cap at 1.
    for name in ["a.txt", "b.txt", "c.txt"] {
        commit_file(tmp.path(), name, "line\n", "add");
        std::fs::write(tmp.path().join(name), "line\nmore\n").unwrap();
    }
    let output = bounded_show_changes_output(tmp.path(), true, 1, 80);
    assert_eq!(
        output["hunk_count"].as_u64(),
        Some(1),
        "must cap at max_hunks"
    );
    assert_eq!(
        output["hunks_truncated"], true,
        "must report hunk truncation"
    );
    let reasons = output["truncation_reasons"].as_array().unwrap();
    assert!(reasons.iter().any(|r| r == "diff_hunk_count_limit"));
    assert!(!reasons.iter().any(|r| r == "diff_hunk_line_limit"));
    assert!(!reasons.iter().any(|r| r == "diff_byte_budget"));
}

#[test]
fn show_changes_diff_respects_max_hunk_lines() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    // One file with many changed lines -> one big hunk; cap lines at 3.
    let content = (0..20).map(|i| format!("line{i}\n")).collect::<String>();
    commit_file(tmp.path(), "big.txt", &content, "initial");
    std::fs::write(
        tmp.path().join("big.txt"),
        format!("{content}extra\nmore\n"),
    )
    .unwrap();
    let output = bounded_show_changes_output(tmp.path(), true, 20, 3);
    assert_eq!(output["hunk_count"].as_u64(), Some(1));
    let files = output["hunks"].as_array().unwrap();
    let hunks = files[0]["hunks"].as_array().unwrap();
    assert!(!hunks.is_empty(), "expected at least one hunk: {files:?}");
    let lines = hunks[0]["diff"].as_str().unwrap().lines().count();
    // header line + up to 3 content lines = at most 4 lines.
    assert!(lines <= 4, "hunk must be line-bounded: {hunks:?}");
    assert_eq!(output["hunks_truncated"], true);
    let reasons = output["truncation_reasons"].as_array().unwrap();
    assert!(reasons.iter().any(|r| r == "diff_hunk_line_limit"));
    assert!(!reasons.iter().any(|r| r == "diff_hunk_count_limit"));
    assert!(!reasons.iter().any(|r| r == "diff_byte_budget"));
}

#[test]
fn show_changes_large_diff_does_not_depend_on_transport_tail() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let big = (0..50).map(|i| format!("line{i}\n")).collect::<String>();
    commit_file(tmp.path(), "big.txt", &big, "initial");
    // Make a diff with many hunks; the command bounds to max_hunks=1, line=10.
    let modified = (0..50)
        .map(|i| {
            if i % 2 == 0 {
                format!("mod{i}\n")
            } else {
                format!("line{i}\n")
            }
        })
        .collect::<String>();
    std::fs::write(tmp.path().join("big.txt"), modified).unwrap();
    let output = bounded_show_changes_output(tmp.path(), true, 1, 10);
    // The first selected hunk must be present and the diff must be bounded.
    assert_eq!(output["hunk_count"].as_u64(), Some(1));
    // The structured fields (not a tail marker) report truncation.
    assert_eq!(output["hunks_truncated"], true);
    let serialized = serde_json::to_string(&output).unwrap();
    assert!(
        !serialized.contains("[output truncated"),
        "must not rely on transport tail marker: {serialized}"
    );
}

#[test]
fn show_changes_schema_covers_truncation_and_transport_fields() {
    let schema = crate::tool_runtime::registry::output_schema_for_tool("show_changes");
    let properties = schema["properties"]["output"]["properties"]
        .as_object()
        .expect("show_changes output properties");
    for field in [
        "files_total",
        "files_returned",
        "files_truncated",
        "files_limit",
        "transport_safe",
        "output_budget_bytes",
        "output_truncated",
        "truncation_reasons",
    ] {
        assert!(
            properties.contains_key(field),
            "missing truncation/transport field {field}"
        );
    }
}

#[tokio::test]
async fn show_changes_include_diff_agent_command_does_not_enqueue_python_helper() {
    let runtime = runtime_with_agent_project("show-native");
    let caps = ShellClientCapabilities {
        shell: true,
        internal_posix_script: true,
        ..Default::default()
    };
    register_agent(&runtime, "show-native", None, caps).await;
    let project = agent_test_project_id("show-native");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::ShowChanges {
                        project,
                        session_id: None,
                        include_diff: Some(true),
                        max_hunks: None,
                        max_hunk_lines: None,
                        session_event_limit: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "show-native")
        .await
        .expect("show_changes should enqueue an agent shell request");
    assert_eq!(req.kind, "run_internal_posix_script");
    assert!(req.command.is_empty());
    let payload = req
        .script
        .as_ref()
        .expect("show_changes must carry a typed internal script");
    assert_eq!(
        payload.language,
        crate::shell_protocol::ShellScriptLanguage::Sh
    );
    assert!(payload.args.is_empty());
    let forbidden = ["python3", "-c"].join(" ");
    assert!(
        !payload.script.contains(&forbidden),
        "show_changes include_diff must not enqueue a Python helper: {}",
        payload.script
    );
    assert!(payload.script.contains("git diff --unified=80"));
    // Modern frame layout: status, status-result, head, head-meta, stat,
    // stat-meta, diff, diff-meta (with diff_exit=0 so success is provable).
    let stdout = "## main\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nstatus_exit=0\nrepository_probe=inside_worktree\nrepository_probe_exit=0\nfiles_total=0\nfiles_returned=0\nfiles_truncated=0\nfiles_limit=200\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nabc123\0abc123\0head\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nhead_exit=0\nhead_truncated=0\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n\n@@WEBCODEX_SHOW_CHANGES_SEP@@\ndiff_stat_exit=0\ndiff_stat_truncated=0\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n\n@@WEBCODEX_SHOW_CHANGES_SEP@@\ndiff_exit=0\ndiff_hunks_returned=0\ndiff_hunks_truncated=0\ndiff_trunc_hunk_count=0\ndiff_trunc_hunk_lines=0\ndiff_trunc_bytes=0\ndiff_bytes=0\n";
    complete_patch_agent_request(&runtime, "show-native", &req.request_id, 0, stdout, "").await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["untracked_previews"], json!([]));
}

#[test]
fn show_changes_clean_worktree() {
    let output = parse_show_changes_output(
            "agent:oe:webcodex",
            "## main...origin/main",
            "b47e4fb000000000000000000000000000000000\0b47e4fb\0fix: route anchor edit file ops through agent dispatch",
            "",
            None,
            20,
            80,
            Some(0),
            "",
        );
    assert_eq!(output["clean"], true);
    assert_eq!(output["branch"], "main");
    assert_eq!(output["head"]["short"], "b47e4fb");
    assert_eq!(output["counts"]["modified"], 0);
    assert!(output["files"].as_array().unwrap().is_empty());
    assert!(output.get("hunks").is_none());
    assert!(output["session"].is_null());
    assert_eq!(output["suggested_next_actions"][0], "no changes detected");
    assert_review_verdict_shape(&output["verdict"]);
    assert_ne!(output["verdict"]["status"], "fail");
    assert_eq!(output["verdict"]["blocking"], false);
}

#[test]
fn show_changes_without_session_id_treats_dirty_workspace_as_advisory() {
    let mut output = parse_show_changes_output(
        "agent:oe:webcodex",
        "## main\n M src/lib.rs",
        "b47e4fb000000000000000000000000000000000\0b47e4fb\0fix",
        " src/lib.rs | 2 +-",
        None,
        20,
        80,
        Some(0),
        "",
    );
    apply_show_changes_session(&mut output, None, None);
    assert_eq!(output["clean"], false);
    assert_eq!(output["counts"]["modified"], 1);
    assert_review_verdict_shape(&output["verdict"]);
    assert_eq!(output["verdict"]["status"], "warn");
    assert_eq!(output["verdict"]["blocking"], false);
    assert_reason_list_contains(&output["verdict"], "warning_reasons", "workspace_dirty");
    assert!(output["session"].is_null());
    assert!(output["suggested_next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "review diff"));
}

#[test]
fn show_changes_with_session_id_includes_session_summary() {
    let runtime = test_runtime();
    let session = runtime.sessions.start_session(
        Some("agent:oe:webcodex".to_string()),
        Some("finish task".to_string()),
    );
    let write_args = json!({"project": "agent:oe:webcodex", "path": "src/foo.rs"});
    let write = runtime.sessions.record_tool_call_started(
        Some(&session.session_id),
        crate::tool_runtime::sessions::SessionTransport::Api,
        "write_project_file",
        &write_args,
    );
    runtime
        .sessions
        .record_tool_call_finished(write, true, &json!({}), None, None);
    let shell_args = json!({"project": "agent:oe:webcodex", "command": "cargo test"});
    let shell = runtime.sessions.record_tool_call_started(
        Some(&session.session_id),
        crate::tool_runtime::sessions::SessionTransport::Api,
        "run_shell",
        &shell_args,
    );
    runtime
        .sessions
        .record_tool_call_finished(shell, true, &json!({}), None, None);

    let mut output = parse_show_changes_output(
        "agent:oe:webcodex",
        "## main\n M src/foo.rs",
        "b47e4fb000000000000000000000000000000000\0b47e4fb\0fix",
        " src/foo.rs | 2 +-",
        None,
        20,
        80,
        Some(0),
        "",
    );
    let summary = runtime.sessions.summary(&session.session_id, Some(30));
    apply_show_changes_session(&mut output, Some(&session.session_id), summary);

    assert_eq!(output["session"]["found"], true);
    assert_eq!(output["session"]["session_id"], session.session_id);
    assert_eq!(output["session"]["title"], "finish task");
    assert_eq!(output["session"]["counts"]["tool_calls"], 2);
    assert_eq!(output["session"]["counts"]["write_like"], 1);
    assert_eq!(output["session"]["counts"]["shell_like"], 1);
    assert_eq!(output["session"]["changed_paths"], json!(["src/foo.rs"]));
    assert!(output["session"]["recent_events"].as_array().unwrap().len() >= 2);
    let actions = output["suggested_next_actions"].as_array().unwrap();
    assert!(actions
        .iter()
        .any(|v| v == "review changed paths from this session"));
    assert!(actions
        .iter()
        .any(|v| v == "check command/test results before commit"));
}

#[test]
fn show_changes_with_missing_session_id_returns_warning_not_panic() {
    let mut output = parse_show_changes_output(
        "agent:oe:webcodex",
        "## main",
        "b47e4fb000000000000000000000000000000000\0b47e4fb\0fix",
        "",
        None,
        20,
        80,
        Some(0),
        "",
    );
    apply_show_changes_session(&mut output, Some("wc_sess_missing"), None);
    assert_eq!(output["session"]["found"], false);
    assert_eq!(output["session"]["session_id"], "wc_sess_missing");
    assert!(output["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["kind"] == "session_not_found"));
    assert_eq!(output["suggested_next_actions"][0], "no changes detected");
}

#[test]
fn show_changes_session_changed_paths_are_deduped() {
    let runtime = test_runtime();
    let session = runtime.sessions.start_session(None, None);
    for path in ["src/foo.rs", "src/foo.rs", "src/bar.rs"] {
        let args = json!({"project": "agent:oe:webcodex", "path": path});
        let start = runtime.sessions.record_tool_call_started(
            Some(&session.session_id),
            crate::tool_runtime::sessions::SessionTransport::Api,
            "write_project_file",
            &args,
        );
        runtime
            .sessions
            .record_tool_call_finished(start, true, &json!({}), None, None);
    }
    let mut output = parse_show_changes_output(
        "agent:oe:webcodex",
        "## main\n M src/foo.rs",
        "b47e4fb000000000000000000000000000000000\0b47e4fb\0fix",
        " src/foo.rs | 2 +-",
        None,
        20,
        80,
        Some(0),
        "",
    );
    let summary = runtime.sessions.summary(&session.session_id, Some(30));
    apply_show_changes_session(&mut output, Some(&session.session_id), summary);
    assert_eq!(
        output["session"]["changed_paths"],
        json!(["src/foo.rs", "src/bar.rs"])
    );
}

#[tokio::test]
async fn show_changes_session_event_limit_is_bounded() {
    let runtime = runtime_with_agent_project("show");
    let caps = ShellClientCapabilities {
        shell: true,
        internal_posix_script: true,
        ..Default::default()
    };
    register_agent(&runtime, "show", None, caps).await;
    let session = runtime.sessions.start_session(None, None);
    for idx in 0..250 {
        let args =
            json!({"project": agent_test_project_id("show"), "path": format!("src/{idx}.rs")});
        let start = runtime.sessions.record_tool_call_started(
            Some(&session.session_id),
            crate::tool_runtime::sessions::SessionTransport::Api,
            "write_project_file",
            &args,
        );
        runtime
            .sessions
            .record_tool_call_finished(start, true, &json!({}), None, None);
    }
    let runtime_for_task = runtime.clone();
    let project = agent_test_project_id("show");
    let session_id = session.session_id.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .show_changes(project, Some(session_id), None, None, None, Some(999))
            .await
    });
    let req = next_patch_agent_request(&runtime, "show")
        .await
        .expect("show_changes should enqueue an agent shell request");
    let stdout = "## main\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nstatus_exit=0\nrepository_probe=inside_worktree\nrepository_probe_exit=0\nfiles_total=0\nfiles_returned=0\nfiles_truncated=0\nfiles_limit=200\nmodified=0\nadded=0\ndeleted=0\nrenamed=0\ncopied=0\nuntracked=0\nconflicted=0\nstaged=0\nunstaged=0\nstatus_trunc_count=0\nstatus_trunc_bytes=0\nstatus_trunc_path=0\nstatus_bytes=7\n@@WEBCODEX_SHOW_CHANGES_SEP@@\ncommit=abc123\nshort=abc123\nsummary=head\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nhead_exit=0\nhead_truncated=0\nhead_bytes=39\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n\n@@WEBCODEX_SHOW_CHANGES_SEP@@\ndiff_stat_exit=0\ndiff_stat_truncated=0\ndiff_stat_bytes=0\n";
    complete_patch_agent_request(&runtime, "show", &req.request_id, 0, stdout, "").await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    let len = result.output["session"]["recent_events"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(len, 200);
}

#[test]
fn show_changes_reports_modified_file() {
    let output = parse_show_changes_output(
        "agent:oe:webcodex",
        "## main\n M src/users_http.rs",
        "b47e4fb000000000000000000000000000000000\0b47e4fb\0fix",
        " src/users_http.rs | 2 +-\n 1 file changed, 1 insertion(+), 1 deletion(-)",
        None,
        20,
        80,
        Some(0),
        "",
    );
    assert_eq!(output["clean"], false);
    assert_eq!(output["counts"]["modified"], 1);
    assert_eq!(output["counts"]["unstaged"], 1);
    assert_eq!(output["verdict"]["status"], "warn");
    assert_reason_list_contains(&output["verdict"], "warning_reasons", "workspace_dirty");
    assert_eq!(output["files"][0]["path"], "src/users_http.rs");
    assert_eq!(output["files"][0]["status"], "modified");
    assert_eq!(output["files"][0]["kind"], "tracked");
    assert!(output["diff_stat"]
        .as_str()
        .unwrap()
        .contains("1 file changed"));
}

#[test]
fn show_changes_reports_untracked_file() {
    let output = parse_show_changes_output(
        "agent:oe:webcodex",
        "## main\n?? webcodex-anchor-edit-smoke-c99f7de.txt",
        "b47e4fb000000000000000000000000000000000\0b47e4fb\0fix",
        "",
        None,
        20,
        80,
        Some(0),
        "",
    );
    assert_eq!(output["clean"], false);
    assert_eq!(output["counts"]["untracked"], 1);
    assert_eq!(output["counts"]["conflicted"], 0);
    assert_eq!(output["files"][0]["status"], "untracked");
    assert_eq!(output["files"][0]["staged"], false);
    assert_eq!(output["warnings"][0]["kind"], "untracked_smoke_file");
    assert_eq!(output["verdict"]["status"], "warn");
    assert_reason_list_contains(&output["verdict"], "warning_reasons", "workspace_dirty");
    assert!(output["suggested_next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str().unwrap().contains("untracked")));
}

#[test]
fn show_changes_reports_conflicted_file() {
    let output = parse_show_changes_output(
        "agent:oe:webcodex",
        "## main\nUU conflicted.rs",
        "b47e4fb000000000000000000000000000000000\0b47e4fb\0fix",
        "",
        None,
        20,
        80,
        Some(0),
        "",
    );
    assert_eq!(output["clean"], false);
    assert_eq!(output["counts"]["conflicted"], 1);
    assert_eq!(output["counts"]["modified"], 0);
    assert_eq!(output["files"][0]["path"], "conflicted.rs");
    assert_eq!(output["files"][0]["status"], "conflicted");
    assert_eq!(output["files"][0]["kind"], "conflicted");
    assert!(
        output["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["kind"] == "workspace_conflicts"),
        "expected workspace_conflicts warning: {}",
        output["warnings"]
    );
    // A real merge conflict is a hard blocker; ordinary dirty state is advisory.
    assert_eq!(output["verdict"]["status"], "fail");
    assert_reason_list_contains(
        &output["verdict"],
        "blocking_reasons",
        "workspace_conflicts",
    );
    assert_reason_list_contains(&output["verdict"], "warning_reasons", "workspace_dirty");
}

#[test]
fn show_changes_include_diff_false_omits_hunks() {
    let output = parse_show_changes_output(
        "agent:oe:webcodex",
        "## main\n M src/lib.rs",
        "b47e4fb000000000000000000000000000000000\0b47e4fb\0fix",
        " src/lib.rs | 2 +-",
        None,
        20,
        80,
        Some(0),
        "",
    );
    assert!(output.get("hunks").is_none());
    assert!(output.get("hunk_count").is_none());
}

#[test]
fn show_changes_include_diff_true_returns_bounded_hunks() {
    let diff = "\
diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 line one
-old
+new
 line three
@@ -10,3 +10,3 @@
 alpha
-beta
+gamma
 omega
";
    let output = parse_show_changes_output(
        "agent:oe:webcodex",
        "## main\n M src/lib.rs",
        "b47e4fb000000000000000000000000000000000\0b47e4fb\0fix",
        " src/lib.rs | 4 ++--",
        Some(diff),
        1,
        4,
        Some(0),
        "",
    );
    assert_eq!(output["hunk_count"], 1);
    assert_eq!(output["hunks_truncated"], true);
    assert_reason_list_contains(&output["verdict"], "warning_reasons", "truncated_by_limit");
    let hunks = output["hunks"].as_array().unwrap();
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0]["path"], "src/lib.rs");
    assert_eq!(hunks[0]["hunks"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn show_changes_clean_repo_include_diff_false_has_no_untracked_previews() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());

    let output = show_changes_output_from_command(tmp.path(), false);

    assert_eq!(output["clean"], true);
    assert_eq!(output["counts"]["untracked"], 0);
    assert!(output.get("untracked_previews").is_none());
}

#[tokio::test]
async fn show_changes_untracked_text_include_diff_false_omits_preview() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let content = "webcodex untracked preview body";
    fs::write(tmp.path().join("notes.txt"), content).unwrap();

    let output = show_changes_output_from_command(tmp.path(), false);

    assert_eq!(output["counts"]["untracked"], 1);
    assert!(output_has_file(&output, "notes.txt"));
    assert!(output.get("untracked_previews").is_none());
    let serialized = serde_json::to_string(&output).unwrap();
    assert!(
        !serialized.contains(content),
        "include_diff=false leaked untracked file content: {serialized}"
    );
}

#[tokio::test]
async fn show_changes_untracked_text_include_diff_true_returns_bounded_preview() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    fs::write(tmp.path().join("notes.txt"), "alpha\nbeta\n").unwrap();

    let output = show_changes_output_from_command(tmp.path(), true);

    assert_eq!(output["counts"]["untracked"], 1);
    assert!(output_has_file(&output, "notes.txt"));
    let preview = preview_for_path(&output, "notes.txt");
    assert_eq!(preview["kind"], "text");
    assert_eq!(preview["line_count"], 2);
    assert_eq!(preview["truncated"], false);
    assert_eq!(preview["lines"][0]["line"], 1);
    assert_eq!(preview["lines"][0]["text"], "alpha");
    assert_eq!(preview["lines"][1]["line"], 2);
    assert_eq!(preview["lines"][1]["text"], "beta");
    assert_eq!(output["hunk_count"], 0);
}

#[tokio::test]
async fn show_changes_untracked_large_file_preview_is_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    fs::write(tmp.path().join("large.txt"), vec![b'x'; 8193]).unwrap();

    let output = show_changes_output_from_command(tmp.path(), true);

    assert_eq!(output["counts"]["untracked"], 1);
    let preview = preview_for_path(&output, "large.txt");
    assert_eq!(preview["kind"], "skipped");
    assert_eq!(preview["reason"], "too_large");
    assert_eq!(preview["byte_count"], 8193);
}

#[tokio::test]
async fn show_changes_untracked_binary_preview_is_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    fs::write(tmp.path().join("binary.bin"), [0, 159, 146, 150]).unwrap();

    let output = show_changes_output_from_command(tmp.path(), true);

    assert_eq!(output["counts"]["untracked"], 1);
    let preview = preview_for_path(&output, "binary.bin");
    assert_eq!(preview["kind"], "skipped");
    assert_eq!(preview["reason"], "binary_or_non_utf8");
}

#[tokio::test]
async fn show_changes_untracked_sensitive_path_preview_is_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    fs::write(tmp.path().join("agent.toml"), "API_TOKEN=secret\n").unwrap();

    let output = show_changes_output_from_command(tmp.path(), true);

    assert_eq!(output["counts"]["untracked"], 1);
    let preview = preview_for_path(&output, "agent.toml");
    assert_eq!(preview["kind"], "skipped");
    assert_eq!(preview["reason"], "sensitive_or_excluded_path");
    let serialized = serde_json::to_string(&output).unwrap();
    assert!(
        !serialized.contains("API_TOKEN=secret"),
        "sensitive file content leaked: {serialized}"
    );
    assert_verdict_omits_raw_output_and_sensitive_values(
        &output["verdict"],
        &["API_TOKEN=secret"],
        "show_changes sensitive preview verdict",
    );
}

#[test]
fn git_diff_hunks_command_rejects_unsafe_paths() {
    assert!(git_diff_hunks_command(&["src/lib.rs".to_string()], false)
        .unwrap()
        .contains("git diff --unified=80 -- 'src/lib.rs'"));
    assert!(validate_project_relative_path("../outside").is_err());
}

#[tokio::test]
async fn git_diff_hunks_rejects_unsafe_paths_before_project_dispatch() {
    let runtime = test_runtime();
    let result = runtime
        .git_diff_hunks(
            "agent:oe:webcodex".to_string(),
            Some(vec!["../outside".to_string()]),
            None,
            None,
            None,
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("parent traversal"));
}

#[tokio::test]
async fn show_changes_with_session_id_returns_session_block_and_records_call() {
    let runtime = runtime_with_agent_project("telemetry-show");
    let caps = ShellClientCapabilities {
        file_read: true,
        shell: true,
        internal_posix_script: true,
        ..Default::default()
    };
    register_agent(&runtime, "telemetry-show", None, caps).await;
    let project = agent_test_project_id("telemetry-show");
    let session = runtime.sessions.start_session(None, None);

    let read_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFile {
                        project,
                        path: "README.md".to_string(),
                        session_id: Some(session_id),
                        start_line: None,
                        limit: Some(1),
                        with_line_numbers: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_agent_request_for_instance(&runtime, "telemetry-show", "inst")
        .await
        .expect("read_file should enqueue before show_changes");
    complete_patch_agent_request(
        &runtime,
        "telemetry-show",
        &req.request_id,
        0,
        &canonical_agent_file_read_range("hello\n", 1, 1),
        "",
    )
    .await;
    let read = read_task.await.unwrap();
    assert!(read.success, "{:?}", read.error);

    let show_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::ShowChanges {
                        project,
                        session_id: Some(session_id),
                        include_diff: Some(false),
                        max_hunks: None,
                        max_hunk_lines: None,
                        session_event_limit: Some(20),
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "telemetry-show")
        .await
        .expect("show_changes should enqueue shell request");
    let stdout = "## main\n M README.md\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nstatus_exit=0\nrepository_probe=inside_worktree\nrepository_probe_exit=0\nfiles_total=1\nfiles_returned=1\nfiles_truncated=0\nfiles_limit=200\nmodified=1\nadded=0\ndeleted=0\nrenamed=0\ncopied=0\nuntracked=0\nconflicted=0\nstaged=0\nunstaged=1\nstatus_trunc_count=0\nstatus_trunc_bytes=0\nstatus_trunc_path=0\nstatus_bytes=20\n@@WEBCODEX_SHOW_CHANGES_SEP@@\ncommit=abc123\nshort=abc123\nsummary=head\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nhead_exit=0\nhead_truncated=0\nhead_bytes=39\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nREADME.md | 1 +\n@@WEBCODEX_SHOW_CHANGES_SEP@@\ndiff_stat_exit=0\ndiff_stat_truncated=0\ndiff_stat_bytes=15\n";
    complete_patch_agent_request(&runtime, "telemetry-show", &req.request_id, 0, stdout, "").await;
    let result = show_task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["session_recorded"], true);
    assert_eq!(result.output["session"]["found"], true);
    assert_eq!(result.output["session"]["counts"]["tool_calls"], 1);
    assert!(result.output["session"]["recent_events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["tool_name"] == "read_file"));
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.tool_calls, 2);
    assert_eq!(summary.counts.change_summary_like, 1);
    let event = finished_event(&summary, "show_changes");
    assert!(event.git_like);
    assert!(event.change_summary_like);
}

#[tokio::test]
async fn show_changes_accepts_unique_short_id() {
    let runtime = runtime_with_resolver_projects().await;
    let bootstrap = auth_context(None, true);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ShowChanges {
                        project: "other-repo".to_string(),
                        session_id: None,
                        include_diff: Some(false),
                        max_hunks: None,
                        max_hunk_lines: None,
                        session_event_limit: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_agent_request_for_client(&runtime, "workstation")
        .await
        .expect("show_changes should enqueue an agent shell request");
    assert_eq!(req.cwd.as_deref(), Some("/root/git/workstation-other-repo"));
    let stdout = "## main\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nstatus_exit=0\nrepository_probe=inside_worktree\nrepository_probe_exit=0\nfiles_total=0\nfiles_returned=0\nfiles_truncated=0\nfiles_limit=200\nmodified=0\nadded=0\ndeleted=0\nrenamed=0\ncopied=0\nuntracked=0\nconflicted=0\nstaged=0\nunstaged=0\nstatus_trunc_count=0\nstatus_trunc_bytes=0\nstatus_trunc_path=0\nstatus_bytes=7\n@@WEBCODEX_SHOW_CHANGES_SEP@@\ncommit=abc123\nshort=abc123\nsummary=head\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nhead_exit=0\nhead_truncated=0\nhead_bytes=39\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n\n@@WEBCODEX_SHOW_CHANGES_SEP@@\ndiff_stat_exit=0\ndiff_stat_truncated=0\ndiff_stat_bytes=0\n";
    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "workstation".to_string(),
            agent_instance_id: "inst-workstation".to_string(),
            request_id: req.request_id,
            exit_code: Some(0),
            stdout: Some(stdout.to_string()),
            stderr: Some(String::new()),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["project"], "other-repo");
}

#[test]
fn parse_porcelain_summary_buckets_untracked_files() {
    let summary =
        parse_porcelain_summary(" M README.md\n?? tmp.txt\nR  old.rs -> new.rs\n!! ignored.log\n");
    assert_eq!(summary.tracked_changed_files, vec!["README.md", "new.rs"]);
    assert_eq!(summary.untracked_files, vec!["tmp.txt"]);
    assert_eq!(summary.ignored_files, vec!["ignored.log"]);
    assert_eq!(summary.changed_files_count, 4);
}

#[test]
fn parse_porcelain_summary_handles_basic_rename_and_quoted_paths() {
    let porcelain =
        " M src/main.rs\nA  new_file.rs\nR  old_name.rs -> new_name.rs\n?? \"quoted path.rs\"";
    let files = parse_porcelain_summary(porcelain).changed_files;
    assert_eq!(
        files,
        vec![
            "src/main.rs",
            "new_file.rs",
            "new_name.rs",
            "quoted path.rs",
        ]
    );
}

#[test]
fn split_diff_summary_separates_porcelain_and_stat() {
    let stdout = format!(
        " M src/a.rs\nA  src/b.rs\n\n{}\n src/a.rs | 2 +-\n 1 file changed",
        DIFF_SUMMARY_SENTINEL,
    );
    let (porcelain, diff_stat) = split_diff_summary(&stdout);
    assert!(porcelain.contains("src/a.rs"));
    assert!(porcelain.contains("src/b.rs"));
    assert!(!porcelain.contains(DIFF_SUMMARY_SENTINEL));
    assert!(diff_stat.contains("1 file changed"));
    assert!(!diff_stat.contains(DIFF_SUMMARY_SENTINEL));
}

#[test]
fn split_diff_summary_without_sentinel_returns_all_as_porcelain() {
    let (porcelain, diff_stat) = split_diff_summary("just status lines");
    assert_eq!(porcelain, "just status lines");
    assert_eq!(diff_stat, "");
}

#[test]
fn git_log_command_is_read_only_and_bounded() {
    assert_eq!(normalize_git_log_limit(None), 20);
    assert_eq!(normalize_git_log_limit(Some(0)), 20);
    assert_eq!(normalize_git_log_limit(Some(999)), 100);
    assert_eq!(normalize_git_log_skip(Some(20_000)), 10_000);
    let cmd = git_log_command(21, 7);
    assert!(cmd.contains("git log"));
    assert!(cmd.contains("-n 22"));
    assert!(cmd.contains("--skip 7"));
    for forbidden in [
        "apply", "commit", "checkout", "reset", "push", "stash", "merge", "rebase", "rm ",
    ] {
        assert!(
            !cmd.contains(forbidden),
            "git_log command must not contain '{}': {}",
            forbidden,
            cmd
        );
    }
}

#[test]
fn git_log_parser_splits_commits_refs_and_truncation() {
    let stdout = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\u{1f}aaaaaaa\u{1f}HEAD -> main, tag: v1\u{1f}Ada\u{1f}ada@example.com\u{1f}2026-06-30T00:00:00+00:00\u{1f}newest\u{1e}bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\u{1f}bbbbbbb\u{1f}\u{1f}Ben\u{1f}ben@example.com\u{1f}2026-06-29T00:00:00+00:00\u{1f}older\u{1e}";
    let (commits, truncated) = parse_git_log_commits(stdout, 1);
    assert!(truncated);
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0]["short_hash"], "aaaaaaa");
    assert_eq!(commits[0]["subject"], "newest");
    assert_eq!(commits[0]["refs"], json!(["HEAD", "main", "v1"]));
}

#[test]
fn git_diff_summary_command_is_read_only() {
    let cmd = git_diff_summary_command();
    // Must run only read-only git inspection subcommands.
    assert!(cmd.contains("git status --porcelain"));
    assert!(cmd.contains("git diff --stat"));
    // No mutating subcommands may appear.
    for forbidden in [
        "apply", "commit", "checkout", "reset", "push", "stash", "merge", "rebase", "rm ",
    ] {
        assert!(
            !cmd.contains(forbidden),
            "git_diff_summary command must not contain '{}': {}",
            forbidden,
            cmd
        );
    }
}

#[tokio::test]
async fn git_diff_summary_agent_uses_internal_posix_runtime() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "before\n", "initial");
    std::fs::write(tmp.path().join("README.md"), "after\n").unwrap();

    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "summary-internal", "demo", tmp.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move { runtime.git_diff_summary(project).await }
    });

    let request = next_patch_agent_request(&runtime, "summary-internal")
        .await
        .expect("git_diff_summary should enqueue one internal request");
    assert_eq!(request.kind, "run_internal_posix_script");
    assert!(request.command.is_empty());
    let payload = request
        .script
        .as_ref()
        .expect("git_diff_summary must carry a typed internal script");
    assert_eq!(
        payload.language,
        crate::shell_protocol::ShellScriptLanguage::Sh
    );
    assert_eq!(payload.script, git_diff_summary_command());
    assert!(payload.args.is_empty());
    complete_agent_request_by_running_locally(&runtime, "summary-internal", request).await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["changed_files_count"], 1);
    assert_eq!(result.output["changed_files"], json!(["README.md"]));
    assert!(result.output["diff_stat"]
        .as_str()
        .unwrap()
        .contains("README.md"));
}

#[tokio::test]
async fn git_or_shell_tools_rejected_without_git_or_shell_capability() {
    let runtime = runtime_with_agent_project("oe");
    register_agent(
        &runtime,
        "oe",
        None,
        ShellClientCapabilities {
            shell: false,
            ..Default::default()
        },
    )
    .await;
    let bootstrap = auth_context(None, true);

    let calls = [
        ToolCall::GitDiffSummary {
            project: agent_test_project_id("oe"),
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
    for _ in 0..20 {
        if task.is_finished() {
            break;
        }
        if let Some(req) = next_patch_agent_request(runtime, client_id).await {
            assert_eq!(req.kind, "run_internal_posix_script");
            assert!(req.command.is_empty());
            let payload = req
                .script
                .as_ref()
                .expect("show_changes must carry a typed internal script");
            assert_eq!(
                payload.language,
                crate::shell_protocol::ShellScriptLanguage::Sh
            );
            assert!(payload.args.is_empty());
            complete_agent_request_by_running_locally(runtime, client_id, req).await;
        } else {
            tokio::task::yield_now().await;
        }
    }
    assert!(
        task.is_finished(),
        "show_changes did not finish after agent requests"
    );
    task.await.unwrap()
}

fn framed_block(kind: char, body: &str, metadata: &str) -> String {
    format!(
        "{body}{metadata}WCSF1:{kind}:{:010}:{:010}\n",
        body.len(),
        metadata.len()
    )
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
            if include_diff { 4 } else { 3 }
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
    }

    let (_, valid, stderr) = run_bounded_show_changes_full(tmp.path(), false, 20, 80);
    let trailer = valid.rfind("WCSF1:T:").unwrap();
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
        "{}{}{}",
        framed_block('S', "## main\n", "status_exit=0\n"),
        framed_block('H', "", "head_exit=1\n"),
        framed_block('T', "", "diff_stat_exit=0\n")
    );
    assert!(split_show_changes_stdout(&synthetic, false).framing_valid);
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
        register_agent_project_at_path(&runtime, "show-collision", "demo", tmp.path()).await;
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
        register_agent_project_at_path(&runtime, "show-untracked", "demo", tmp.path()).await;
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

    let tmp = tempfile::tempdir().unwrap();
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
    let schema = crate::tool_runtime::registry::output_schema_for_tool("show_changes");
    let envelope = json!({
        "success": result.success,
        "output": result.output,
        "error": result.error,
    });
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&envelope, &schema)
        .unwrap_or_else(|error| {
            panic!("{label} show_changes schema mismatch: {error}\n{envelope}")
        });
}

/// Validate a bare `show_changes` output `Value` against the output schema by
/// wrapping it in the success envelope.
fn assert_show_changes_envelope_value_matches_schema(output: &Value, label: &str) {
    let schema = crate::tool_runtime::registry::output_schema_for_tool("show_changes");
    let envelope = json!({
        "success": true,
        "output": output,
        "error": Value::Null,
    });
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&envelope, &schema)
        .unwrap_or_else(|error| {
            panic!("{label} show_changes schema mismatch: {error}\n{envelope}")
        });
}

/// Run the production-bounded `show_changes` command in a local repo and
/// parse it into the structured `show_changes` output value, the same way the
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

/// Mirror the Runner/Shell transport's 256 KiB tail-retention behavior: when
/// output exceeds `max_bytes`, keep only the last `max_bytes` and prefix it
/// with the transport's truncation marker. Used to prove the production-side
/// bounding keeps the protocol frame intact past the transport cap.
fn simulate_transport_tail(stdout: &str, max_bytes: usize) -> String {
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
    use std::process::Command;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(_) => return (-1, String::new(), "failed to spawn".to_string()),
    };
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
    let project = register_agent_project_at_path(&runtime, "ng", "demo", tmp.path()).await;
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
        "git_unavailable",
    );
    let actions = result.output["suggested_next_actions"].as_array().unwrap();
    assert!(actions
        .iter()
        .any(|a| a.as_str().unwrap().contains("unavailable")));
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
    let project = register_agent_project_at_path(&runtime, "ngs", "demo", tmp.path()).await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("task".to_string()));
    let args = json!({"project": project, "path": "src/foo.rs"});
    let start = runtime.sessions.record_tool_call_started(
        Some(&session.session_id),
        crate::tool_runtime::sessions::SessionTransport::Api,
        "write_project_file",
        &args,
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
    assert!(!result.output["session"]["recent_events"]
        .as_array()
        .unwrap()
        .is_empty());
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
    let project = register_agent_project_at_path(&runtime, "gr", "demo", tmp.path()).await;
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
    let project = register_agent_project_at_path(&runtime, "grd", "demo", tmp.path()).await;
    let result = run_show_changes_via_agent(&runtime, "grd", project, None, true).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["status_observation"]["status"], "observed");
    assert_eq!(result.output["clean"], false);
    assert!(result.output["hunk_count"].as_u64().unwrap_or(0) > 0);
    assert_show_changes_envelope_matches_schema("git include_diff=true", &result);
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
    let project = register_agent_project_at_path(&runtime, "gsf", "demo", tmp.path()).await;
    let result = run_show_changes_via_agent(&runtime, "gsf", project, None, false).await;
    assert!(!result.success, "status failure must fail show_changes");
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
            "b47e4fb000000000000000000000000000000000\0b47e4fb\0head",
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
            "b47e4fb000000000000000000000000000000000\0b47e4fb\0head",
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
    assert_eq!(output["upstream_reason_code"], "git_unavailable");
    assert_eq!(output["upstream"], Value::Null);
    assert_eq!(output["ahead"], Value::Null);
    assert_eq!(output["behind"], Value::Null);
    assert_reason_list_contains(&output["verdict"], "warning_reasons", "git_unavailable");
}

#[test]
fn show_changes_schema_explicitly_models_upstream_and_nullable_observation() {
    let schema = crate::tool_runtime::registry::output_schema_for_tool("show_changes");
    let properties = schema["properties"]["output"]["properties"]
        .as_object()
        .expect("show_changes output properties");
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

/// Helper: run the bounded show_changes command with full stdout/stderr capture
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
        "show_changes failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
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

    let mut rejected_seen = false;
    for (i, path) in paths.iter().enumerate() {
        let display = path.to_string_lossy();
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

/// Parse already-captured frames into the structured show_changes output,
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

    let tmp = tempfile::tempdir().unwrap();
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

    let wrapper_dir = tempfile::tempdir().unwrap();
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
    let project = register_agent_project_at_path(&runtime, "stat-only", "demo", repo.path()).await;
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
        let req = next_patch_agent_request(&runtime, "stat-only")
            .await
            .expect("show_changes should enqueue a shell request");
        assert_eq!(req.kind, "run_internal_posix_script");
        assert!(req.command.is_empty());
        let payload = req
            .script
            .as_ref()
            .expect("show_changes must carry a typed internal script");
        assert_eq!(
            payload.language,
            crate::shell_protocol::ShellScriptLanguage::Sh
        );
        assert!(payload.args.is_empty());
        assert!(
            payload.script.len() <= crate::shell_protocol::RAW_SHELL_WIRE_MAX_BYTES,
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
        register_agent_project_at_path(&runtime, "stat-missing", "demo", repo.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .show_changes(project, None, Some(false), None, None, None)
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "stat-missing")
        .await
        .expect("show_changes should enqueue a shell request");
    assert_eq!(req.kind, "run_internal_posix_script");
    assert!(req.command.is_empty());
    let payload = req
        .script
        .as_ref()
        .expect("show_changes must carry a typed internal script");
    let (command_exit, stdout, stderr) = run_command_full_capture(&payload.script, repo.path(), 30);
    assert_eq!(command_exit, 0, "show_changes command failed: {stderr}");
    let mut missing_stat_exit = stdout
        .lines()
        .filter(|line| !line.starts_with("diff_stat_exit="))
        .collect::<Vec<_>>()
        .join("\n");
    missing_stat_exit.push('\n');
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
    let project = register_agent_project_at_path(&runtime, "extd", "demo", tmp.path()).await;
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
    let req = next_patch_agent_request(&runtime, "extd")
        .await
        .expect("show_changes should enqueue a shell request");
    // Run the generated internal script locally with the failing external diff
    // in the environment so the full `git diff` fails.
    assert_eq!(req.kind, "run_internal_posix_script");
    assert!(req.command.is_empty());
    let payload = req
        .script
        .as_ref()
        .expect("show_changes must carry a typed internal script");
    let full = format!("{ext_env} {}", payload.script);
    let (exit, stdout, stderr) = run_command_full_capture(&full, tmp.path(), 30);
    complete_patch_agent_request(&runtime, "extd", &req.request_id, exit, &stdout, &stderr).await;
    let result = task.await.unwrap();
    // The full git diff failed, so show_changes must report a tool failure even
    // though the production-side diff filter succeeded.
    assert!(
        !result.success,
        "show_changes must fail when the full git diff fails: {:?}",
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

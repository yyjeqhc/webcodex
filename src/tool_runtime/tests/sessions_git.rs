//! Git-related session tests for tool_runtime.

use super::super::*;
use super::support::*;
use crate::runner_protocol::RunnerCapabilities;
use crate::tool_runtime::git::{
    git_log_next_skip, normalize_git_log_limit, normalize_git_log_skip,
};
use serde_json::json;

#[tokio::test]
async fn git_status_with_session_id_records_git_read_event() {
    let runtime = runtime_with_agent_project("telemetry-git");
    let caps = RunnerCapabilities {
        git: true,
        shell: false,
        ..Default::default()
    };
    register_agent(&runtime, "telemetry-git", None, caps).await;
    let project = agent_test_project_id("telemetry-git");
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::GitStatus {
                        project,
                        session_id: Some(session_id),
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = wait_for_patch_agent_request(&runtime, "telemetry-git").await;
    complete_patch_agent_request(&runtime, "telemetry-git", &req.request_id, 0, "", "").await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert!(result.output.get("session_recorded").is_none());
    assert!(result.output.get("session_event_id").is_none());
    assert!(result.output.get("session_id").is_none());
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.tool_calls, 1);
    assert_eq!(summary.counts.read_like, 1);
    assert_eq!(summary.counts.git_like, 1);
    let event = finished_event(&summary, "git_status");
    assert!(event.git_like);
    assert!(event.read_like);
}

#[tokio::test]
async fn git_log_parses_commits() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    commit_file(root, "a.txt", "one\n", "first commit");
    commit_file(root, "a.txt", "two\n", "second commit");
    let stdout = git_log_stdout(root, 20, 0);
    let runtime = runtime_with_agent_project("git-log-parse");
    let caps = RunnerCapabilities {
        git: true,
        ..Default::default()
    };
    register_agent(&runtime, "git-log-parse", None, caps).await;
    let project = agent_test_project_id("git-log-parse");

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::GitLog {
                        project,
                        head_commit: None,
                        limit: None,
                        skip: None,
                        session_id: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = wait_for_patch_agent_request(&runtime, "git-log-parse").await;
    assert_eq!(req.kind, "run_internal_posix_script");
    assert!(req.command.is_empty());
    let script = req
        .script
        .as_ref()
        .expect("git_log must use a typed POSIX script");
    assert_eq!(script.language.as_str(), "sh");
    assert!(script.script.contains("git log"));
    assert!(script.script.contains("-n 21"));
    complete_patch_agent_request(&runtime, "git-log-parse", &req.request_id, 0, &stdout, "").await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["project"], project);
    assert_eq!(result.output["limit"], 20);
    assert_eq!(result.output["skip"], 0);
    assert_eq!(result.output["count"], 2);
    assert_eq!(result.output["truncated"], false);
    assert_eq!(result.output["next_skip"], serde_json::Value::Null);
    let commits = result.output["commits"].as_array().unwrap();
    assert_eq!(commits[0]["subject"], "second commit");
    assert!(commits[0]["hash"].as_str().is_some_and(|s| s.len() >= 40));
    assert!(commits[0]["short_hash"]
        .as_str()
        .is_some_and(|s| !s.is_empty()));
    assert!(commits[0]["author_date"]
        .as_str()
        .is_some_and(|s| !s.is_empty()));
    assert_eq!(commits[0]["author_name"], "WebCodex Test");
    assert_eq!(commits[0]["author_email"], "webcodex-test@example.com");
    assert!(commits[0]["refs"].as_array().is_some());
}

#[tokio::test]
async fn git_log_limit_and_skip_returns_second_recent_and_truncated() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    commit_file(root, "a.txt", "one\n", "first commit");
    commit_file(root, "a.txt", "two\n", "second commit");
    commit_file(root, "a.txt", "three\n", "third commit");
    let stdout = git_log_stdout(root, 1, 1);
    let runtime = runtime_with_agent_project("git-log-page");
    let caps = RunnerCapabilities {
        git: true,
        ..Default::default()
    };
    register_agent(&runtime, "git-log-page", None, caps).await;
    let project = agent_test_project_id("git-log-page");

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::GitLog {
                        project,
                        head_commit: None,
                        limit: Some(1),
                        skip: Some(1),
                        session_id: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = wait_for_patch_agent_request(&runtime, "git-log-page").await;
    assert_eq!(req.kind, "run_internal_posix_script");
    assert!(req.command.is_empty());
    let script = req
        .script
        .as_ref()
        .expect("git_log must use a typed POSIX script");
    assert_eq!(script.language.as_str(), "sh");
    assert!(script.script.contains("-n 2"));
    assert!(script.script.contains("--skip 1"));
    complete_patch_agent_request(&runtime, "git-log-page", &req.request_id, 0, &stdout, "").await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["limit"], 1);
    assert_eq!(result.output["skip"], 1);
    assert_eq!(result.output["count"], 1);
    assert_eq!(result.output["truncated"], true);
    assert_eq!(result.output["next_skip"], 2);
    let commits = result.output["commits"].as_array().unwrap();
    assert_eq!(commits[0]["subject"], "second commit");
}

async fn run_git_log_page(
    client_id: &str,
    root: &std::path::Path,
    limit: usize,
    skip: usize,
) -> ToolResult {
    run_git_log_page_with_stdout(client_id, git_log_stdout(root, limit, skip), limit, skip).await
}

async fn run_git_log_page_with_stdout(
    client_id: &str,
    stdout: String,
    limit: usize,
    skip: usize,
) -> ToolResult {
    let runtime = runtime_with_agent_project(client_id);
    register_agent(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            git: true,
            ..Default::default()
        },
    )
    .await;
    let project = "agent-proj".to_string();
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::GitLog {
                        project,
                        head_commit: None,
                        limit: Some(limit),
                        skip: Some(skip),
                        session_id: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    assert_eq!(request.kind, "run_internal_posix_script");
    assert!(request.command.is_empty());
    let script = request
        .script
        .as_ref()
        .expect("git_log must use a typed POSIX script");
    assert_eq!(script.language.as_str(), "sh");
    assert!(script.script.contains(&format!("-n {}", limit + 1)));
    assert!(script.script.contains(&format!("--skip {skip}")));
    complete_patch_agent_request(&runtime, client_id, &request.request_id, 0, &stdout, "").await;
    task.await.unwrap()
}

async fn run_git_log_tool_call_with_stdout(
    client_id: &str,
    call: ToolCall,
    stdout: String,
    expected_head: &str,
) -> ToolResult {
    let runtime = runtime_with_agent_project(client_id);
    register_agent(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            git: true,
            ..Default::default()
        },
    )
    .await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime.dispatch_with_auth(call, Some(&bootstrap)).await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    assert_eq!(request.kind, "run_internal_posix_script");
    assert!(request.command.is_empty());
    let script = request
        .script
        .as_ref()
        .expect("git_log must use a typed POSIX script");
    assert_eq!(script.language.as_str(), "sh");
    assert!(script.script.contains(expected_head), "{}", script.script);
    assert!(
        !script.script.contains("HEAD^{commit}"),
        "{}",
        script.script
    );
    complete_patch_agent_request(&runtime, client_id, &request.request_id, 0, &stdout, "").await;
    task.await.unwrap()
}

#[tokio::test]
async fn git_log_retained_tail_never_advertises_a_complete_or_continuable_page() {
    let record = |subject: &str| {
        format!(
            "{}\u{1f}aaaaaaa\u{1f}\u{1f}Ada\u{1f}ada@example.com\u{1f}2026-06-30T00:00:00+00:00\u{1f}{subject}\u{1e}",
            "a".repeat(40)
        )
    };
    // Exercise actual registry retention, which drops the oversized first
    // record's prefix but leaves a perfectly parseable older commit behind.
    let stdout = record(&"x".repeat(300 * 1024))
        + &record("older")
        + &format!("__WEBCODEX_GIT_LOG_HEAD__={}\u{1e}", "a".repeat(40));
    let result = run_git_log_page_with_stdout("git-log-retained-tail", stdout, 2, 0).await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "source_incomplete");
    assert!(result.output.get("next_skip").is_none());
    assert!(result.output.get("commits").is_none());
}

#[tokio::test]
async fn git_log_snapshot_continuation_survives_head_advance_without_gap_or_duplicate() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    for index in 1..=5 {
        commit_file(
            root,
            "paged.txt",
            &format!("{index}\n"),
            &format!("commit {index}"),
        );
    }

    let first = run_git_log_page("git-log-multipage", root, 2, 0).await;
    assert!(first.success, "{:?}", first.error);
    assert_eq!(first.output["truncated"], true);
    assert_eq!(first.output["next_skip"], 2);
    let snapshot = first.output["head_commit"].as_str().unwrap().to_string();
    assert_eq!(snapshot.len(), 40);
    let first_next = &first.output["suggested_call"];
    assert_eq!(first_next["tool"], "git_log");
    assert_eq!(
        first_next["arguments"]["project"],
        agent_test_project_id("git-log-multipage")
    );
    assert_eq!(first_next["arguments"]["head_commit"], snapshot);
    assert_eq!(first_next["arguments"]["limit"], 2);
    assert_eq!(first_next["arguments"]["skip"], 2);
    let second_call = ToolCall::from_tool_name(
        first_next["tool"].as_str().unwrap(),
        first_next["arguments"].clone(),
    )
    .expect("git_log suggested_call must be parser-ready");
    let mut projected_first = ToolResult::ok(first.output.clone());
    crate::model_surface::project_tool_result_suggested_calls(
        "git_log",
        &mut projected_first,
        &|target| crate::model_surface::suggested_tool_call_route(target, false),
    );
    let projected_next = &projected_first.output["suggested_call"];
    assert_eq!(
        projected_next["tool"],
        crate::model_surface::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME
    );
    assert_eq!(projected_next["arguments"]["tool"], "git_log");
    assert_eq!(
        projected_next["arguments"]["arguments"],
        first_next["arguments"]
    );

    commit_file(root, "paged.txt", "6\n", "commit 6");
    let current_head = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(current_head.status.success());
    assert_ne!(
        String::from_utf8_lossy(&current_head.stdout).trim(),
        snapshot
    );

    let second_stdout = git_log_stdout_at_head(root, &snapshot, 2, 2);
    let second = run_git_log_tool_call_with_stdout(
        "git-log-multipage",
        second_call,
        second_stdout,
        &snapshot,
    )
    .await;
    assert!(second.success, "{:?}", second.error);
    assert_eq!(second.output["head_commit"], snapshot);
    assert_eq!(second.output["truncated"], true);
    assert_eq!(second.output["next_skip"], 4);
    let second_next = &second.output["suggested_call"];
    assert_eq!(second_next["arguments"]["head_commit"], snapshot);
    assert_eq!(second_next["arguments"]["skip"], 4);
    let final_call = ToolCall::from_tool_name(
        second_next["tool"].as_str().unwrap(),
        second_next["arguments"].clone(),
    )
    .expect("second git_log suggested_call must be parser-ready");
    let final_stdout = git_log_stdout_at_head(root, &snapshot, 2, 4);
    let final_page =
        run_git_log_tool_call_with_stdout("git-log-multipage", final_call, final_stdout, &snapshot)
            .await;
    assert!(final_page.success, "{:?}", final_page.error);
    assert_eq!(final_page.output["head_commit"], snapshot);
    assert_eq!(final_page.output["truncated"], false);
    assert_eq!(final_page.output["next_skip"], serde_json::Value::Null);
    assert!(final_page.output.get("suggested_call").is_none());

    let subjects = first.output["commits"]
        .as_array()
        .unwrap()
        .iter()
        .chain(second.output["commits"].as_array().unwrap())
        .chain(final_page.output["commits"].as_array().unwrap())
        .map(|commit| commit["subject"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        subjects,
        vec!["commit 5", "commit 4", "commit 3", "commit 2", "commit 1"]
    );
    assert!(!subjects.iter().any(|subject| subject == "commit 6"));
    let unique = subjects.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(unique.len(), subjects.len());

    assert_eq!(normalize_git_log_limit(Some(usize::MAX)), 100);
    assert_eq!(normalize_git_log_skip(Some(usize::MAX)), 10_000);
    assert_eq!(git_log_next_skip(10_000, 1, true), None);
}

#[tokio::test]
async fn git_log_missing_exact_snapshot_fails_closed_without_head_fallback() {
    let runtime = runtime_with_agent_project("git-log-missing-snapshot");
    register_agent(
        &runtime,
        "git-log-missing-snapshot",
        None,
        RunnerCapabilities {
            git: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("git-log-missing-snapshot");
    let missing = "f".repeat(40);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let missing_for_call = missing.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::GitLog {
                        project,
                        head_commit: Some(missing_for_call),
                        limit: Some(2),
                        skip: Some(2),
                        session_id: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, "git-log-missing-snapshot").await;
    assert_eq!(request.kind, "run_internal_posix_script");
    assert!(request.command.is_empty());
    let script = request
        .script
        .as_ref()
        .expect("git_log must use a typed POSIX script");
    assert_eq!(script.language.as_str(), "sh");
    assert!(script.script.contains(&missing));
    assert!(!script.script.contains("HEAD^{commit}"));
    complete_patch_agent_request(
        &runtime,
        "git-log-missing-snapshot",
        &request.request_id,
        42,
        "",
        "git log snapshot unavailable\n",
    )
    .await;
    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "snapshot_unavailable");
    assert_eq!(result.output["head_commit"], missing);
    assert!(result.output.get("commits").is_none());
    assert!(result.output.get("suggested_call").is_none());
}

#[tokio::test]
async fn git_log_unborn_repository_is_an_empty_final_page() {
    let runtime = runtime_with_agent_project("git-log-unborn");
    register_agent(
        &runtime,
        "git-log-unborn",
        None,
        RunnerCapabilities {
            git: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("git-log-unborn");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::GitLog {
                        project,
                        head_commit: None,
                        limit: Some(20),
                        skip: Some(0),
                        session_id: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, "git-log-unborn").await;
    complete_patch_agent_request(
        &runtime,
        "git-log-unborn",
        &request.request_id,
        0,
        "__WEBCODEX_GIT_LOG_HEAD__=unborn\u{1e}",
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["head_commit"], serde_json::Value::Null);
    assert_eq!(result.output["count"], 0);
    assert_eq!(result.output["commits"], json!([]));
    assert_eq!(result.output["truncated"], false);
    assert_eq!(result.output["next_skip"], serde_json::Value::Null);
}

#[tokio::test]
async fn git_log_unknown_project_and_unknown_session_are_structured_errors() {
    let runtime = runtime_with_agent_project("git-log-errors");
    let caps = RunnerCapabilities {
        git: true,
        ..Default::default()
    };
    register_agent(&runtime, "git-log-errors", None, caps).await;
    let project = agent_test_project_id("git-log-errors");

    let unknown_project = runtime
        .dispatch(ToolCall::GitLog {
            project: "ghost".to_string(),
            head_commit: None,
            limit: None,
            skip: None,
            session_id: None,
        })
        .await;
    assert!(!unknown_project.success);
    assert_eq!(unknown_project.output["error_kind"], "unknown_project");

    let unknown_session = runtime
        .dispatch(ToolCall::GitLog {
            project,
            head_commit: None,
            limit: None,
            skip: None,
            session_id: Some("wc_sess_missing".to_string()),
        })
        .await;
    assert!(!unknown_session.success);
    assert_eq!(unknown_session.output["error_kind"], "unknown_session_id");
}

#[tokio::test]
async fn git_log_read_only_session_allowed_and_recorded() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    commit_file(root, "a.txt", "one\n", "first commit");
    commit_file(root, "a.txt", "two\n", "second commit");
    let stdout = git_log_stdout(root, 1, 0);
    let runtime = runtime_with_agent_project("git-log-readonly");
    let caps = RunnerCapabilities {
        git: true,
        ..Default::default()
    };
    register_agent(&runtime, "git-log-readonly", None, caps).await;
    let project = agent_test_project_id("git-log-readonly");
    let bootstrap = auth_context(None, true);
    let session_result = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name(
                "start_session",
                json!({"project": project, "mode": "read_only"}),
            )
            .unwrap(),
            Some(&bootstrap),
        )
        .await;
    assert!(session_result.success, "{:?}", session_result.error);
    let session_id = session_result.output["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session_id.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::GitLog {
                        project,
                        head_commit: None,
                        limit: Some(1),
                        skip: Some(0),
                        session_id: Some(session_id),
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = wait_for_patch_agent_request(&runtime, "git-log-readonly").await;
    complete_patch_agent_request(
        &runtime,
        "git-log-readonly",
        &req.request_id,
        0,
        &stdout,
        "",
    )
    .await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(
        result.output["suggested_call"]["arguments"]["session_id"],
        session_id
    );
    assert!(ToolCall::from_tool_name(
        result.output["suggested_call"]["tool"].as_str().unwrap(),
        result.output["suggested_call"]["arguments"].clone(),
    )
    .is_ok());
    assert!(result.output.get("session_recorded").is_none());
    assert!(result.output.get("session_event_id").is_none());
    assert!(result.output.get("session_id").is_none());
    let summary = runtime.sessions.summary(&session_id, Some(20)).unwrap();
    assert_eq!(summary.counts.tool_calls, 1);
    assert_eq!(summary.counts.read_like, 1);
    assert_eq!(summary.counts.git_like, 1);
    let event = finished_event(&summary, "git_log");
    assert!(event.read_like);
    assert!(event.git_like);
    assert!(!event.write_like);
}

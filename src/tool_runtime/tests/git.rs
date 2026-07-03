//! Git tests for tool_runtime.

use super::super::git::*;
use super::super::helpers::*;
use super::super::types::*;
use super::support::*;
use crate::shell_protocol::{ShellAgentResultRequest, ShellClientCapabilities};
use crate::tool_runtime::ToolRuntime;
use serde_json::json;
use std::fs;

#[test]
fn git_diff_hunks_tool_is_known_and_schema_is_bounded() {
    assert!(KNOWN_TOOL_NAMES.contains(&"git_diff_hunks"));
    let call = ToolCall::from_tool_name(
        "git_diff_hunks",
        json!({
            "project":"agent:oe:webcodex",
            "paths":["src/runtime_http.rs"],
            "max_hunks":20,
            "max_hunk_lines":120,
            "cached":true
        }),
    )
    .unwrap();
    assert!(matches!(
        call,
        ToolCall::GitDiffHunks { project, cached: Some(true), .. }
            if project == "agent:oe:webcodex"
    ));

    let runtime = test_runtime();
    let specs = runtime.tool_specs();
    let spec = spec_named(&specs, "git_diff_hunks");
    let props = spec.input_schema["properties"].as_object().unwrap();
    for field in ["project", "paths", "max_hunks", "max_hunk_lines", "cached"] {
        assert!(props.contains_key(field), "missing {}", field);
    }
    let output_props = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    for field in ["files", "hunk_count", "truncated", "exit_code", "stderr"] {
        assert!(output_props.contains_key(field), "missing {}", field);
    }
}

#[test]
fn show_changes_tool_is_known_and_parses() {
    assert!(KNOWN_TOOL_NAMES.contains(&"show_changes"));
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
    let without_diff = show_changes_command(false);
    let with_diff = show_changes_command(true);
    for cmd in [without_diff, with_diff] {
        assert!(cmd.contains("git status --porcelain=v1 -b"));
        assert!(cmd.contains("git log -1"));
        assert!(cmd.contains("git diff --stat"));
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

#[tokio::test]
async fn show_changes_include_diff_agent_command_does_not_enqueue_python_helper() {
    let runtime = runtime_with_agent_project("show-native");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
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
    let forbidden = ["python3", "-c"].join(" ");
    assert!(
        !req.command.contains(&forbidden),
        "show_changes include_diff must not enqueue a Python helper: {}",
        req.command
    );
    assert!(req.command.contains("git diff --unified=80"));
    let stdout = "## main\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nabc123\0abc123\0head\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n";
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
}

#[test]
fn show_changes_without_session_id_keeps_existing_behavior() {
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
        "replace_line_range",
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
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
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
    let stdout = "## main\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nabc123\0abc123\0test head\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n";
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
    assert_eq!(output["files"][0]["status"], "untracked");
    assert_eq!(output["files"][0]["staged"], false);
    assert_eq!(output["warnings"][0]["kind"], "untracked_smoke_file");
    assert!(output["suggested_next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str().unwrap().contains("untracked")));
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
    let mut caps = ShellClientCapabilities::default();
    caps.file_read = true;
    caps.shell = true;
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
        "hello\n",
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
    let stdout =
            "## main\n M README.md\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nabc123\0abc123\0head\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n README.md | 1 +\n";
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
    let stdout = "## main\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nabc123\0abc123\0head\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n";
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
fn parse_porcelain_files_handles_basic_rename_and_quoted_paths() {
    let porcelain =
        " M src/main.rs\nA  new_file.rs\nR  old_name.rs -> new_name.rs\n?? \"quoted path.rs\"";
    let files = parse_porcelain_files(porcelain);
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
async fn git_or_shell_tools_rejected_without_git_or_shell_capability() {
    let runtime = runtime_with_agent_project("oe");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = false; // git stays false by default
    register_agent(&runtime, "oe", None, caps).await;
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
fn is_non_git_project_inspection_detects_english_and_localized_fatal() {
    // English git fatal message.
    assert!(is_non_git_project_inspection(
        Some(128),
        "fatal: not a git repository (or any of the parent directories): .git",
        "",
    ));
    // Localized git fatal message (no English substring) — caught by the
    // locale-independent structural signal.
    assert!(is_non_git_project_inspection(
        Some(128),
        "fatal: 不是 git 仓库（或者任何父目录）：.git",
        "",
    ));
    // Locale-independent structural signal: non-zero exit and no `## ` branch
    // header in the porcelain status output.
    assert!(is_non_git_project_inspection(
        Some(129),
        "usage: git diff --no-index ...",
        "",
    ));
    // Real git repo: exit 0 (not flagged regardless of stderr).
    assert!(!is_non_git_project_inspection(Some(0), "", "## main"));
    // Real git repo with no commits still emits a `## ` header.
    assert!(!is_non_git_project_inspection(
        Some(0),
        "",
        "## No commits yet on main",
    ));
    // Non-zero exit but a real repo produced a branch header (some other git
    // issue): not classified as non-git.
    assert!(!is_non_git_project_inspection(
        Some(1),
        "some other error",
        "## main\n M src/lib.rs",
    ));
}

async fn run_show_changes_via_agent(
    runtime: &ToolRuntime,
    client_id: &str,
    project: String,
    session_id: Option<String>,
) -> ToolResult {
    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .show_changes(project, session_id, None, None, None, None)
            .await
    });
    let req = next_patch_agent_request(runtime, client_id)
        .await
        .expect("show_changes should enqueue an agent shell request");
    complete_agent_request_by_running_locally(runtime, client_id, req).await;
    task.await.unwrap()
}

#[tokio::test]
async fn show_changes_degrades_gracefully_for_non_git_project() {
    let tmp = tempfile::tempdir().unwrap();
    // Intentionally do NOT init a git repo.
    let runtime = test_runtime();
    let project = register_agent_project_at_path(&runtime, "ng", "demo", tmp.path()).await;
    let result = run_show_changes_via_agent(&runtime, "ng", project, None).await;
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
    let actions = result.output["suggested_next_actions"].as_array().unwrap();
    assert!(actions
        .iter()
        .any(|a| a.as_str().unwrap().contains("unavailable")));
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

    let result =
        run_show_changes_via_agent(&runtime, "ngs", project, Some(session.session_id.clone()))
            .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["non_git_project"], true);
    assert_eq!(result.output["git_available"], false);
    assert_eq!(result.output["session"]["found"], true);
    assert_eq!(result.output["session"]["session_id"], session.session_id);
    assert!(
        result.output["session"]["recent_events"]
            .as_array()
            .unwrap()
            .len()
            >= 1
    );
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
    let result = run_show_changes_via_agent(&runtime, "gr", project, None).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["non_git_project"], false);
    assert_eq!(result.output["git_available"], true);
    assert_eq!(result.output["git_error"], serde_json::Value::Null);
    assert_eq!(result.output["clean"], true);
    assert!(result.output["branch"].as_str().is_some());
    assert!(result.output["head"]["short"].as_str().is_some());
    assert_eq!(result.output["counts"]["modified"], 0);
    assert!(result.output["files"].as_array().unwrap().is_empty());
    // No git-unavailable suggestion for a real repo.
    let actions = result.output["suggested_next_actions"].as_array().unwrap();
    assert!(!actions
        .iter()
        .any(|a| a.as_str().unwrap().contains("unavailable")));
}

//! Files tests for tool_runtime.

use super::super::cargo::*;
use super::super::codex::*;
use super::super::files::*;
use super::super::git::*;
use super::super::helpers::*;
use super::super::patch::*;
use super::super::types::*;
use super::super::*;
use super::support::*;
use crate::projects::{Executor, ProjectConfig, ProjectsConfig, ProjectsState};
use crate::shell_client::ShellClientRegistry;
use crate::shell_protocol::{
    AgentPolicySummary, ShellAgentPollRequest, ShellAgentProjectSummary, ShellAgentResultRequest,
    ShellAgentShellRequest, ShellClientCapabilities, ShellClientRegisterRequest,
};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[tokio::test]
async fn write_project_file_with_session_id_records_changed_path_without_content() {
    let runtime = runtime_with_agent_project("telemetry-write");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "telemetry-write", None, caps).await;
    let project = agent_test_project_id("telemetry-write");
    let session = runtime.sessions.start_session(None, None);
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
                        expected_sha256: None,
                        expected_content_prefix: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "telemetry-write")
        .await
        .expect("write_project_file should enqueue helper request");
    complete_patch_agent_request(
        &runtime,
        "telemetry-write",
        &req.request_id,
        0,
        r#"{"path":"src/new.txt","bytes_written":24,"sha256":"abc","changed":true}"#,
        "",
    )
    .await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.write_like, 1);
    let event = finished_event(&summary, "write_project_file");
    assert!(event.write_like);
    assert_eq!(event.changed_paths, vec!["src/new.txt".to_string()]);
    let serialized = serde_json::to_string(&summary.events).unwrap();
    assert!(
        !serialized.contains("do-not-log-this-content"),
        "session event leaked write content: {serialized}"
    );
}

#[tokio::test]
async fn validate_patch_never_enqueues_mutating_apply_command() {
    let runtime = runtime_with_agent_project("patcher");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "patcher", None, caps).await;

    let project = agent_test_project_id("patcher");
    let marker = "ZZZ_PATCH_MARKER_VALIDATE_ZZZ";
    let patch = marker_patch("VALIDATE_MARKER.md", marker);
    let runtime_for_task = runtime.clone();
    let patch_for_task = patch.clone();
    let validate_task = tokio::spawn(async move {
        runtime_for_task
            .validate_patch(project, patch_for_task, None)
            .await
    });

    // 1) `git apply --check -` (read-only applicability test).
    let check_req = next_patch_agent_request(&runtime, "patcher")
        .await
        .expect("validate_patch should enqueue a check request");
    assert_safe_patch_command(&check_req.command, marker);
    assert_eq!(check_req.command, "git apply --check -");
    assert_ne!(check_req.command, "git apply -");
    assert_eq!(check_req.stdin.as_deref(), Some(patch.as_str()));
    complete_patch_agent_request(&runtime, "patcher", &check_req.request_id, 0, "", "").await;

    // 2) `git apply --stat -` (read-only summary).
    let stat_req = next_patch_agent_request(&runtime, "patcher")
        .await
        .expect("validate_patch should enqueue a stat request");
    assert_safe_patch_command(&stat_req.command, marker);
    assert_eq!(stat_req.command, "git apply --stat -");
    complete_patch_agent_request(&runtime, "patcher", &stat_req.request_id, 0, "stat", "").await;

    // 3) No mutating apply must be enqueued — validate_patch is dry-run only.
    let leaked_apply = next_patch_agent_request(&runtime, "patcher").await;
    assert!(
        leaked_apply.is_none(),
        "validate_patch enqueued a mutating command (got: {:?})",
        leaked_apply.map(|r| r.command)
    );

    let result = validate_task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["can_apply"], true);
}

#[test]
fn validate_preflight_path_rejects_boundary_escapes() {
    // In-bounds relative paths are accepted.
    assert!(validate_preflight_path("README.md").is_ok());
    assert!(validate_preflight_path("src/main.rs").is_ok());
    // Absolute paths, traversal, empty, and NUL are hard rejects.
    assert!(validate_preflight_path("").is_err());
    assert!(validate_preflight_path("/etc/passwd").is_err());
    assert!(validate_preflight_path("../outside").is_err());
    assert!(validate_preflight_path("src/../../outside").is_err());
    assert!(validate_preflight_path("src\0main.rs").is_err());
    // Sensitive filenames are NOT hard-rejected here (they become warnings).
    assert!(validate_preflight_path(".env").is_ok());
    assert!(validate_preflight_path("agent.toml").is_ok());
}

#[test]
fn sensitive_path_warnings_flags_sensitive_names() {
    assert!(sensitive_path_warnings(".env")
        .iter()
        .any(|w| w.contains(".env")));
    assert!(sensitive_path_warnings("config/agent.toml")
        .iter()
        .any(|w| w.contains("agent.toml")));
    assert!(sensitive_path_warnings("webcodex.env")
        .iter()
        .any(|w| w.contains("webcodex.env")));
    assert!(sensitive_path_warnings("projects.d/x.toml")
        .iter()
        .any(|w| w.contains("projects.d")));
    assert!(sensitive_path_warnings(".git/config")
        .iter()
        .any(|w| w.contains(".git")));
    assert!(sensitive_path_warnings("target/debug/x")
        .iter()
        .any(|w| w.contains("target")));
    assert!(sensitive_path_warnings("node_modules/x")
        .iter()
        .any(|w| w.contains("node_modules")));
    // A normal source file produces no warnings.
    assert!(sensitive_path_warnings("src/main.rs").is_empty());
    // Matching is case-insensitive.
    assert!(sensitive_path_warnings("TARGET/foo")
        .iter()
        .any(|w| w.contains("target")));
}

#[tokio::test]
async fn validate_patch_rejects_empty_patch() {
    let runtime = test_runtime();
    let result = runtime
        .validate_patch("agent:c:p".to_string(), "".to_string(), None)
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("empty"));
}

#[tokio::test]
async fn validate_patch_rejects_nul_byte_patch() {
    let runtime = test_runtime();
    let result = runtime
        .validate_patch("agent:c:p".to_string(), "diff\0--- a/f\n".to_string(), None)
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("NUL"));
}

#[tokio::test]
async fn validate_patch_rejects_oversized_patch() {
    let runtime = test_runtime();
    // Build a patch one byte over the limit.
    let oversized = "x".repeat(MAX_VALIDATE_PATCH_BYTES + 1);
    let result = runtime
        .validate_patch("agent:c:p".to_string(), oversized, None)
        .await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("too large"), "got: {}", err);
}

#[tokio::test]
async fn validate_patch_rejects_non_agent_project() {
    // A server-configured (local) project is not a supported runtime
    // surface for validate_patch. resolve_project rejects it before the
    // agent dry-run path, and the server never reads the filesystem.
    let runtime = test_runtime();
    let patch = "--- a/README.md\n+++ b/README.md\n@@ -1 +1,2 @@\nhello\n+world\n";
    let result = runtime
        .validate_patch("agent:nope:nope".to_string(), patch.to_string(), None)
        .await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(
        err.to_lowercase().contains("unknown") || err.to_lowercase().contains("agent"),
        "expected a routing/rejection error for non-agent project, got: {}",
        err
    );
}

#[test]
fn max_validate_patch_bytes_is_conservative() {
    // Pin the conservative upper bound so it is not accidentally raised.
    assert_eq!(MAX_VALIDATE_PATCH_BYTES, 256 * 1024);
    assert!(MAX_VALIDATE_PATCH_BYTES <= 1024 * 1024);
}

#[test]
fn parse_file_list_entries_is_bounded_and_marks_truncation() {
    // Simulate agent file_list stdout: dirs suffixed with '/'.
    let stdout = "Cargo.toml\nsrc/\nREADME.md\ntarget/\nCargo.lock\n";
    // First, without truncation, verify kinds and project-relative paths.
    let (all, truncated_full) = parse_file_list_entries(stdout, ".", 10);
    assert!(!truncated_full);
    assert_eq!(all.len(), 5);
    let src = all.iter().find(|e| e["path"] == "src").expect("src entry");
    assert_eq!(src["kind"], "dir");
    let cargo = all
        .iter()
        .find(|e| e["path"] == "Cargo.toml")
        .expect("Cargo.toml entry");
    assert_eq!(cargo["kind"], "file");

    // With a tight bound, output is truncated and sorted alphabetically.
    let (bounded, truncated) = parse_file_list_entries(stdout, ".", 3);
    assert_eq!(bounded.len(), 3);
    assert!(truncated);
    let paths: Vec<&str> = bounded
        .iter()
        .map(|e| e["path"].as_str().unwrap())
        .collect();
    // Sorted: Cargo.lock, Cargo.toml, README.md come first.
    assert_eq!(paths, vec!["Cargo.lock", "Cargo.toml", "README.md"]);
}

#[test]
fn parse_file_list_entries_prepends_subpath_for_relative_paths() {
    let stdout = "main.rs\nlib.rs\n";
    let (entries, truncated) = parse_file_list_entries(stdout, "src", 10);
    assert!(!truncated);
    let paths: Vec<&str> = entries
        .iter()
        .map(|e| e["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["src/lib.rs", "src/main.rs"]);
}

#[test]
fn validate_project_relative_path_rejects_absolute_and_parent_traversal() {
    assert!(validate_project_relative_path(".").is_ok());
    assert!(validate_project_relative_path("src").is_ok());
    assert!(validate_project_relative_path("src/main.rs").is_ok());
    assert!(validate_project_relative_path("/etc").is_err());
    assert!(validate_project_relative_path("../outside").is_err());
    assert!(validate_project_relative_path("src/../../outside").is_err());
    assert!(validate_project_relative_path("src\0main.rs").is_err());
}

#[test]
fn parse_search_matches_is_bounded_and_strips_dot_slash() {
    let stdout = "./src/main.rs:10:fn main() {}\n./src/lib.rs:3:pub fn x()\n./src/a:1:1\n";
    let (matches, truncated) = parse_search_matches(stdout, 2);
    assert_eq!(matches.len(), 2);
    assert!(truncated);
    assert_eq!(matches[0]["path"], "src/main.rs");
    assert_eq!(matches[0]["line"], 10);
    assert_eq!(matches[0]["preview"], "fn main() {}");
    assert_eq!(matches[1]["path"], "src/lib.rs");
}

#[test]
fn parse_search_matches_skips_lines_without_line_number() {
    // Binary file matches or malformed lines are skipped, not counted.
    let stdout = "binary:file\nsrc/main.rs:5:hit\n";
    let (matches, _truncated) = parse_search_matches(stdout, 10);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["path"], "src/main.rs");
}

#[test]
fn search_project_text_command_excludes_sensitive_dirs_and_bounds_output() {
    let cmd = search_project_text_command("fn main", "src", 25);
    assert!(cmd.contains("--exclude-dir=.git"));
    assert!(cmd.contains("--exclude-dir=target"));
    assert!(cmd.contains("--exclude-dir=node_modules"));
    assert!(cmd.contains("head -n 26"));
    assert!(cmd.contains("grep -rnI"));
}

#[tokio::test]
async fn list_project_files_requires_file_read_capability() {
    let runtime = runtime_with_agent_project("oe");
    // Default capabilities have file_read = false.
    register_agent(&runtime, "oe", None, ShellClientCapabilities::default()).await;
    let bootstrap = auth_context(None, true);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ListProjectFiles {
                project: agent_test_project_id("oe"),
                session_id: None,
                path: None,
                limit: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!result.success);
    assert!(
        result.error.unwrap().contains("file_read"),
        "list_project_files should require file_read capability"
    );
}

#[tokio::test]
async fn search_project_text_requires_shell_capability() {
    let runtime = runtime_with_agent_project("oe");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = false;
    register_agent(&runtime, "oe", None, caps).await;
    let bootstrap = auth_context(None, true);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::SearchProjectText {
                project: agent_test_project_id("oe"),
                pattern: "fn".to_string(),
                session_id: None,
                path: None,
                limit: None,
                context_before: None,
                context_after: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!result.success);
    assert!(
        result.error.unwrap().contains("shell"),
        "search_project_text should require shell capability"
    );
}

#[tokio::test]
async fn list_project_files_rejects_non_agent_project_id() {
    // A bare project id (not agent:<client>:<project>) is not resolved by
    // the runtime surface — proving routing goes through the owning agent.
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ListProjectFiles {
            project: "some-local-id".to_string(),
            session_id: None,
            path: None,
            limit: None,
        })
        .await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("agent") || err.contains("projects.toml"));
}

#[tokio::test]
async fn list_project_files_rejects_absolute_or_parent_paths_before_agent_request() {
    let runtime = runtime_with_agent_project("oe");
    register_agent(
        &runtime,
        "oe",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let bootstrap = auth_context(None, true);
    for path in ["/etc", "../outside"] {
        let result = runtime
            .dispatch_with_auth(
                ToolCall::ListProjectFiles {
                    project: agent_test_project_id("oe"),
                    session_id: None,
                    path: Some(path.to_string()),
                    limit: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success, "path {} should be rejected", path);
        let err = result.error.unwrap();
        assert!(
            err.contains("project-relative") || err.contains("parent traversal"),
            "unexpected error for {}: {}",
            path,
            err
        );
    }
}

#[tokio::test]
async fn search_project_text_rejects_empty_pattern() {
    // Authorization runs before the tool body, so register an agent with
    // shell capability to reach the empty-pattern validation.
    let runtime = runtime_with_agent_project("oe");
    register_agent(
        &runtime,
        "oe",
        None,
        ShellClientCapabilities {
            shell: true,
            ..Default::default()
        },
    )
    .await;
    let bootstrap = auth_context(None, true);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::SearchProjectText {
                project: agent_test_project_id("oe"),
                pattern: "   ".to_string(),
                session_id: None,
                path: None,
                limit: None,
                context_before: None,
                context_after: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("pattern"));
}

#[tokio::test]
async fn search_project_text_rejects_absolute_or_parent_paths_before_agent_request() {
    let runtime = runtime_with_agent_project("oe");
    register_agent(
        &runtime,
        "oe",
        None,
        ShellClientCapabilities {
            shell: true,
            ..Default::default()
        },
    )
    .await;
    let bootstrap = auth_context(None, true);
    for path in ["/etc", "../outside"] {
        let result = runtime
            .dispatch_with_auth(
                ToolCall::SearchProjectText {
                    project: agent_test_project_id("oe"),
                    pattern: "needle".to_string(),
                    session_id: None,
                    path: Some(path.to_string()),
                    limit: None,
                    context_before: None,
                    context_after: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success, "path {} should be rejected", path);
        let err = result.error.unwrap();
        assert!(
            err.contains("project-relative") || err.contains("parent traversal"),
            "unexpected error for {}: {}",
            path,
            err
        );
    }
}

#[test]
fn validate_edit_file_path_rejects_unsafe_and_sensitive_paths() {
    // Safe relative paths accepted.
    assert!(validate_edit_file_path("README.md").is_ok());
    assert!(validate_edit_file_path("src/main.rs").is_ok());
    assert!(validate_edit_file_path("a/b/c.txt").is_ok());
    // Empty / NUL / absolute / traversal rejected.
    assert!(validate_edit_file_path("").is_err());
    assert!(validate_edit_file_path("src\0main.rs").is_err());
    assert!(validate_edit_file_path("/etc/passwd").is_err());
    assert!(validate_edit_file_path("../outside").is_err());
    assert!(validate_edit_file_path("src/../../outside").is_err());
    // Sensitive paths hard-rejected.
    for sensitive in [
        "agent.toml",
        "config/agent.toml",
        "agent.toml.bak",
        "webcodex.env",
        ".env",
        ".env.local",
        "secrets/projects.d/x",
        "projects.d",
        ".git/config",
        "target/debug/bin",
        "node_modules/pkg/index.js",
    ] {
        assert!(
            validate_edit_file_path(sensitive).is_err(),
            "sensitive path should be rejected: {}",
            sensitive
        );
    }
}

#[test]
fn is_sensitive_edit_path_is_component_wise_not_substring() {
    // Component-wise: a filename that merely contains a sensitive token
    // as a substring is NOT rejected.
    assert!(!is_sensitive_edit_path("targeting.md"));
    assert!(!is_sensitive_edit_path("enviroment.rs"));
    assert!(!is_sensitive_edit_path("docs/agent-toml-notes.md"));
    // Exact component matches ARE rejected.
    assert!(is_sensitive_edit_path("target/foo"));
    assert!(is_sensitive_edit_path(".git/HEAD"));
    assert!(is_sensitive_edit_path("node_modules/x"));
    assert!(is_sensitive_edit_path("a/b/.env"));
}

#[test]
fn is_hex_sha256_validates_lowercase_digest() {
    assert!(is_hex_sha256(
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    ));
    assert!(!is_hex_sha256("abc"));
    assert!(!is_hex_sha256(
        "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"
    ));
    assert!(!is_hex_sha256(
        "z3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    ));
}

#[test]
fn replace_line_range_content_replaces_middle_multiline() {
    let (updated, out) = files::apply_line_edit_content(
        "one\ntwo\nthree\nfour\n",
        "src/example.rs",
        files::LineEditOperation::Replace,
        Some(2),
        Some(3),
        None,
        "TWO\nTHREE",
        None,
        None,
    )
    .unwrap();
    assert_eq!(updated, "one\nTWO\nTHREE\nfour\n");
    assert_eq!(out["path"], "src/example.rs");
    assert_eq!(out["start_line"], 2);
    assert_eq!(out["end_line"], 3);
    assert_eq!(out["old_line_count"], 2);
    assert_eq!(out["new_line_count"], 2);
    assert_eq!(out["changed"], true);
}

#[test]
fn replace_line_range_content_rejects_sha_mismatch_without_write() {
    let original = "one\ntwo\nthree\n";
    let err = files::apply_line_edit_content(
        original,
        "src/example.rs",
        files::LineEditOperation::Replace,
        Some(2),
        Some(2),
        None,
        "TWO",
        Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
        None,
    )
    .unwrap_err();
    assert!(err.contains("Rejected before write"));
    assert!(err.contains("expected_old_sha256 mismatch"));
    assert!(err.contains("No files were modified"));
    assert!(err.contains("Retry guidance"));
    assert_eq!(original, "one\ntwo\nthree\n");
}

#[test]
fn line_edit_guard_failure_reports_no_files_modified_and_retry_guidance() {
    let err = files::apply_line_edit_content(
        "one\ntwo\nthree\n",
        "src/example.rs",
        files::LineEditOperation::Insert,
        None,
        None,
        Some(2),
        "inserted",
        None,
        Some("not-the-anchor"),
    )
    .unwrap_err();
    assert!(err.contains("Rejected before write"));
    assert!(err.contains("expected_anchor_prefix mismatch"));
    assert!(err.contains("No files were modified"));
    assert!(err.contains("Retry guidance"));
    assert!(err.contains("read the file again"));
}

#[test]
fn replace_line_range_content_rejects_out_of_range() {
    let err = files::apply_line_edit_content(
        "one\ntwo\n",
        "src/example.rs",
        files::LineEditOperation::Replace,
        Some(2),
        Some(3),
        None,
        "x",
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(err, "invalid line range");
}

#[test]
fn insert_at_line_content_inserts_start_middle_and_eof() {
    let (start, out) = files::apply_line_edit_content(
        "one\ntwo\n",
        "src/example.rs",
        files::LineEditOperation::Insert,
        None,
        None,
        Some(1),
        "zero",
        None,
        None,
    )
    .unwrap();
    assert_eq!(start, "zero\none\ntwo\n");
    assert_eq!(out["line"], 1);
    assert_eq!(out["old_line_count"], 1);

    let (middle, _) = files::apply_line_edit_content(
        "one\ntwo\n",
        "src/example.rs",
        files::LineEditOperation::Insert,
        None,
        None,
        Some(2),
        "middle\n",
        None,
        None,
    )
    .unwrap();
    assert_eq!(middle, "one\nmiddle\ntwo\n");

    let (eof, out) = files::apply_line_edit_content(
        "one\ntwo\n",
        "src/example.rs",
        files::LineEditOperation::Insert,
        None,
        None,
        Some(3),
        "three",
        None,
        None,
    )
    .unwrap();
    assert_eq!(eof, "one\ntwo\nthree\n");
    assert_eq!(out["old_line_count"], 0);
}

#[test]
fn insert_at_line_content_rejects_anchor_prefix_mismatch() {
    let err = files::apply_line_edit_content(
        "one\ntwo\n",
        "src/example.rs",
        files::LineEditOperation::Insert,
        None,
        None,
        Some(2),
        "middle",
        None,
        Some("three"),
    )
    .unwrap_err();
    assert!(err.contains("expected_anchor_prefix mismatch"));
    assert!(err.contains("No files were modified"));
}

#[test]
fn delete_line_range_content_deletes_single_and_multiple_lines() {
    let (single, out) = files::apply_line_edit_content(
        "one\ntwo\nthree\n",
        "src/example.rs",
        files::LineEditOperation::Delete,
        Some(2),
        Some(2),
        None,
        "",
        None,
        None,
    )
    .unwrap();
    assert_eq!(single, "one\nthree\n");
    assert_eq!(out["old_line_count"], 1);
    assert_eq!(out["new_line_count"], 0);

    let (multi, _) = files::apply_line_edit_content(
        "one\ntwo\nthree\nfour\n",
        "src/example.rs",
        files::LineEditOperation::Delete,
        Some(2),
        Some(3),
        None,
        "",
        None,
        None,
    )
    .unwrap();
    assert_eq!(multi, "one\nfour\n");
}

#[test]
fn delete_line_range_content_rejects_out_of_range() {
    let err = files::apply_line_edit_content(
        "one\n",
        "src/example.rs",
        files::LineEditOperation::Delete,
        Some(1),
        Some(2),
        None,
        "",
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(err, "invalid line range");
}

#[tokio::test]
async fn line_edit_tools_reject_oversized_expected_prefix_before_agent_dispatch() {
    let runtime = test_runtime();
    let big_prefix = "x".repeat(MAX_EXPECTED_PREFIX_BYTES + 1);

    let result = runtime
        .replace_line_range(
            "agent:c:p".to_string(),
            "EDIT_PROBE.txt".to_string(),
            1,
            1,
            "new".to_string(),
            None,
            Some(big_prefix.clone()),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("expected prefix too large"));

    let result = runtime
        .insert_at_line(
            "agent:c:p".to_string(),
            "EDIT_PROBE.txt".to_string(),
            1,
            "new".to_string(),
            None,
            Some(big_prefix.clone()),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("expected prefix too large"));

    let result = runtime
        .delete_line_range(
            "agent:c:p".to_string(),
            "EDIT_PROBE.txt".to_string(),
            1,
            1,
            None,
            Some(big_prefix),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("expected prefix too large"));
}

#[tokio::test]
async fn line_edit_dispatch_uses_agent_native_file_op_not_python_helper() {
    let runtime = runtime_with_agent_project("editor");
    register_agent(&runtime, "editor", None, ShellClientCapabilities::default()).await;
    let project = agent_test_project_id("editor");

    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .replace_line_range(
                project,
                "EDIT_PROBE.txt".to_string(),
                2,
                2,
                "new".to_string(),
                None,
                Some("old".to_string()),
            )
            .await
    });

    let mut req = None;
    for _ in 0..20 {
        req = runtime
            .shell_clients
            .poll(ShellAgentPollRequest {
                client_id: "editor".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap();
        if req.is_some() {
            break;
        }
        tokio::task::yield_now().await;
    }
    let req = req.expect("replace_line_range should enqueue an agent file op");
    assert_eq!(req.kind, "file_replace_line_range");
    assert_eq!(req.command, "");
    assert!(req.stdin.is_none());
    assert_eq!(req.path.as_deref(), Some("EDIT_PROBE.txt"));
    assert_eq!(req.content.as_deref(), Some("new"));
    assert_eq!(req.start_line, Some(2));
    assert_eq!(req.end_line, Some(2));
    assert_eq!(req.expected_prefix.as_deref(), Some("old"));

    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "editor".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: req.request_id,
            exit_code: Some(0),
            stdout: Some(
                "{\"changed\":true,\"path\":\"EDIT_PROBE.txt\",\
                     \"old_sha256\":\"b\",\"new_sha256\":\"a\",\
                     \"old_line_count\":1,\"new_line_count\":1,\"bytes_written\":4,\
                     \"start_line\":2,\"end_line\":2}"
                    .to_string(),
            ),
            stderr: Some(String::new()),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["changed"], true);
    assert_eq!(result.output["path"], "EDIT_PROBE.txt");
}

#[tokio::test]
async fn replace_in_file_rejects_invalid_input_before_agent_dispatch() {
    // Call the method directly (bypassing authorize_agent_tool, which would
    // otherwise resolve the project first) so we prove input validation
    // fires before any agent request is enqueued. A test_runtime() has no
    // registered agents, so a request that reached dispatch would hang;
    // these all return early with a validation error.
    let runtime = test_runtime();
    let cases: Vec<(String, String, String)> = vec![
        // empty old
        (
            "EDIT_PROBE.txt".to_string(),
            "".to_string(),
            "x".to_string(),
        ),
        // NUL in old
        (
            "EDIT_PROBE.txt".to_string(),
            "a\0b".to_string(),
            "x".to_string(),
        ),
        // NUL in new
        (
            "EDIT_PROBE.txt".to_string(),
            "a".to_string(),
            "x\0y".to_string(),
        ),
        // sensitive path
        ("agent.toml".to_string(), "a".to_string(), "b".to_string()),
        // absolute path
        ("/etc/passwd".to_string(), "a".to_string(), "b".to_string()),
        // traversal path
        ("../x".to_string(), "a".to_string(), "b".to_string()),
    ];
    for (path, old, new) in cases {
        let result = runtime
            .replace_in_file("agent:c:p".to_string(), path, old, new, None, None)
            .await;
        assert!(!result.success, "expected validation failure");
        let err = result.error.unwrap();
        // Must NOT be the project-resolution error — proves early reject.
        assert!(
            !err.contains("shell client") && !err.contains("projects.toml"),
            "should fail input validation before project resolution: {}",
            err
        );
    }
    // expected_replacements < 1 rejected.
    let result = runtime
        .replace_in_file(
            "agent:c:p".to_string(),
            "EDIT_PROBE.txt".to_string(),
            "a".to_string(),
            "b".to_string(),
            Some(0),
            None,
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("expected_replacements"));

    // expected_replacements > 1 requires allow_multiple=true, otherwise
    // the caller's requested count would be ambiguous.
    let result = runtime
        .replace_in_file(
            "agent:c:p".to_string(),
            "EDIT_PROBE.txt".to_string(),
            "a".to_string(),
            "b".to_string(),
            Some(2),
            Some(false),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("allow_multiple"));
}

#[tokio::test]
async fn write_project_file_rejects_invalid_input_before_agent_dispatch() {
    let runtime = test_runtime();
    // NUL content
    let result = runtime
        .write_project_file(
            "agent:c:p".to_string(),
            "EDIT_PROBE.txt".to_string(),
            "a\0b".to_string(),
            None,
            None,
            None,
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("NUL"));
    // sensitive path
    let result = runtime
        .write_project_file(
            "agent:c:p".to_string(),
            ".env".to_string(),
            "x".to_string(),
            None,
            None,
            None,
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("sensitive"));
    // bad expected_sha256 format
    let result = runtime
        .write_project_file(
            "agent:c:p".to_string(),
            "EDIT_PROBE.txt".to_string(),
            "x".to_string(),
            Some(true),
            Some("not-a-hash".to_string()),
            None,
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("expected_sha256"));
}

#[tokio::test]
async fn replace_in_file_rejects_server_configured_project() {
    // A server-configured (local) project is not an agent-registered
    // runtime surface; replace_in_file must refuse it.
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_local_project(tmp.path(), "demo");
    std::fs::write(tmp.path().join("EDIT_PROBE.txt"), "hello").unwrap();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ReplaceInFile {
                project: "demo".to_string(),
                path: "EDIT_PROBE.txt".to_string(),
                old: "hello".to_string(),
                new: "world".to_string(),
                session_id: None,
                expected_replacements: None,
                allow_multiple: None,
            },
            None,
        )
        .await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(
        err.contains("agent-registered") || err.contains("unknown_project"),
        "should reject server-configured project: {}",
        err
    );
    // File must be unchanged — the server never wrote it.
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
        "hello"
    );
}

#[tokio::test]
async fn replace_in_file_routes_to_owning_agent_with_fixed_helper() {
    let runtime = runtime_with_agent_project("editor");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "editor", None, caps).await;
    let project = agent_test_project_id("editor");

    let runtime_for_task = runtime.clone();
    let project_for_task = project.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .replace_in_file(
                project_for_task,
                "EDIT_PROBE.txt".to_string(),
                "foo".to_string(),
                "bar".to_string(),
                None,
                None,
            )
            .await
    });

    // Drain requests until the helper run arrives.
    let mut req = None;
    for _ in 0..20 {
        req = runtime
            .shell_clients
            .poll(ShellAgentPollRequest {
                client_id: "editor".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap();
        if req.is_some() {
            break;
        }
        tokio::task::yield_now().await;
    }
    let req = req.expect("replace_in_file should enqueue a helper run for the agent");
    // The command is the FIXED python3 helper — no caller content interpolated.
    assert!(
        req.command.starts_with("python3 -c '"),
        "command must be the fixed helper, got: {}",
        req.command
    );
    assert!(
        !req.command.contains("foo") && !req.command.contains("EDIT_PROBE"),
        "caller content must not be interpolated into the command: {}",
        req.command
    );
    // old/new/path travel over stdin as JSON.
    let stdin = req.stdin.expect("helper payload on stdin");
    assert!(stdin.contains("EDIT_PROBE.txt"));
    assert!(stdin.contains("foo"));
    assert!(stdin.contains("bar"));
    assert!(stdin.contains("\"expected_replacements\":1"));
    assert!(stdin.contains("\"allow_multiple\":false"));
    // The agent (server side) never reads the agent fs: respond with a
    // canned JSON result that the runtime forwards verbatim.
    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "editor".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: req.request_id,
            exit_code: Some(0),
            stdout: Some(
                "{\"changed\":true,\"path\":\"EDIT_PROBE.txt\",\"replacements\":1,\
                     \"before_sha256\":\"b\",\"after_sha256\":\"a\",\"bytes_written\":3}"
                    .to_string(),
            ),
            stderr: Some(String::new()),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["changed"], true);
    assert_eq!(result.output["replacements"], 1);
    assert_eq!(result.output["path"], "EDIT_PROBE.txt");
}

#[tokio::test]
async fn edit_tools_rejected_without_required_capability() {
    let runtime = runtime_with_agent_project("editor");
    // ReplaceInFile requires shell; ReplaceLineRange requires file_write.
    // Default caps have shell=true, file_write=false, so register once and
    // test both: ReplaceInFile with shell=false, ReplaceLineRange with defaults.
    register_agent(
        &runtime,
        "editor",
        None,
        ShellClientCapabilities {
            shell: false,
            ..Default::default()
        },
    )
    .await;
    let auth = auth_context(None, true);

    // replace_in_file: requires shell
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ReplaceInFile {
                project: agent_test_project_id("editor"),
                path: "EDIT_PROBE.txt".to_string(),
                old: "foo".to_string(),
                new: "bar".to_string(),
                session_id: None,
                expected_replacements: None,
                allow_multiple: None,
            },
            Some(&auth),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("shell"));

    // line edit tools: requires file_write
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ReplaceLineRange {
                project: agent_test_project_id("editor"),
                path: "EDIT_PROBE.txt".to_string(),
                start_line: 1,
                end_line: 1,
                new_text: "new".to_string(),
                session_id: None,
                expected_old_sha256: None,
                expected_old_prefix: None,
            },
            Some(&auth),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("file_write"));
}

#[test]
fn helper_replace_in_file_single_replacement_success() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("f.txt"), "hello world").unwrap();
    let payload = json!({
        "path": "f.txt",
        "old": "world",
        "new": "rust",
        "expected_replacements": 1,
        "allow_multiple": false
    });
    let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["changed"], true);
    assert_eq!(out["replacements"], 1);
    assert_eq!(out["before_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(out["after_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
        "hello rust"
    );
}

#[test]
fn helper_replace_in_file_old_missing_leaves_file_unchanged() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("f.txt"), "hello world").unwrap();
    let payload = json!({
        "path": "f.txt",
        "old": "missing",
        "new": "x",
        "expected_replacements": 1,
        "allow_multiple": false
    });
    let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["changed"], false);
    assert!(out["error"].as_str().unwrap().contains("not found"));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
        "hello world"
    );
}

#[test]
fn helper_replace_in_file_multiple_without_allow_multiple_fails() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("f.txt"), "a a a").unwrap();
    let payload = json!({
        "path": "f.txt",
        "old": "a",
        "new": "b",
        "expected_replacements": 1,
        "allow_multiple": false
    });
    let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["changed"], false);
    assert!(out["error"].as_str().unwrap().contains("multiple"));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
        "a a a"
    );
}

#[test]
fn helper_replace_in_file_rejects_expected_multiple_without_allow_multiple() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("f.txt"), "hello world").unwrap();
    let payload = json!({
        "path": "f.txt",
        "old": "world",
        "new": "rust",
        "expected_replacements": 2,
        "allow_multiple": false
    });
    let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["changed"], false);
    assert!(out["error"].as_str().unwrap().contains("allow_multiple"));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
        "hello world"
    );
}

#[test]
fn helper_replace_in_file_allow_multiple_exact_count_succeeds() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("f.txt"), "a a a").unwrap();
    let payload = json!({
        "path": "f.txt",
        "old": "a",
        "new": "b",
        "expected_replacements": 3,
        "allow_multiple": true
    });
    let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["changed"], true);
    assert_eq!(out["replacements"], 3);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
        "b b b"
    );
}

#[test]
fn helper_replace_in_file_allow_multiple_count_mismatch_fails() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("f.txt"), "a a a").unwrap();
    let payload = json!({
        "path": "f.txt",
        "old": "a",
        "new": "b",
        "expected_replacements": 2,
        "allow_multiple": true
    });
    let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["changed"], false);
    assert!(out["error"].as_str().unwrap().contains("mismatch"));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
        "a a a"
    );
}

#[test]
fn helper_replace_in_file_rejects_empty_old_and_nul() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("f.txt"), "x").unwrap();
    let payload = json!({
        "path": "f.txt",
        "old": "",
        "new": "y",
        "expected_replacements": 1,
        "allow_multiple": false
    });
    let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["changed"], false);
    assert!(out["error"].as_str().unwrap().contains("old"));
    // File unchanged.
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
        "x"
    );
}

#[test]
fn helper_replace_in_file_rejects_non_utf8_file() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("f.bin"), [0xFF, 0xFE, 0xFD]).unwrap();
    let payload = json!({
        "path": "f.bin",
        "old": "x",
        "new": "y",
        "expected_replacements": 1,
        "allow_multiple": false
    });
    let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["changed"], false);
    assert!(out["error"].as_str().unwrap().contains("UTF-8"));
}

#[test]
fn validate_artifact_file_path_rejects_sensitive_paths() {
    assert!(validate_artifact_file_path("docs/assets/generated.png").is_ok());
    for path in [
        "../evil.png",
        ".git/config",
        ".env",
        "secrets/key.pem",
        "tokens/api.txt",
        "target/out.bin",
        "node_modules/pkg/file",
    ] {
        assert!(
            validate_artifact_file_path(path).is_err(),
            "{} should be rejected",
            path
        );
    }
}

#[test]
fn helper_save_project_artifact_writes_binary_and_blocks_overwrite() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let payload = json!({
        "path": "artifacts/imports/tiny.png",
        "content_base64": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, [0x89, b'P', b'N', b'G']),
        "mime_type": "image/png",
        "overwrite": false,
        "max_bytes": 1024
    });
    let out = run_helper_locally(SAVE_PROJECT_ARTIFACT_HELPER, &payload, tmp.path());
    assert_eq!(out["bytes_written"], 4);
    assert_eq!(out["mime_type"], "image/png");
    assert!(out["sha256"].as_str().unwrap().len() == 64);
    assert_eq!(
        std::fs::read(tmp.path().join("artifacts/imports/tiny.png")).unwrap(),
        vec![0x89, b'P', b'N', b'G']
    );

    let out2 = run_helper_locally(SAVE_PROJECT_ARTIFACT_HELPER, &payload, tmp.path());
    assert!(out2["error"]
        .as_str()
        .unwrap()
        .contains("overwrite is false"));
}

#[test]
fn helper_read_project_artifact_metadata_counts_zip_without_extracting() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("sample.zip");
    let status = std::process::Command::new("python3")
            .arg("-c")
            .arg("import zipfile; z=zipfile.ZipFile('sample.zip','w'); z.writestr('a.txt','a'); z.writestr('b.txt','b'); z.close()")
            .current_dir(tmp.path())
            .status()
            .unwrap();
    assert!(status.success());
    assert!(zip_path.exists());
    let payload = json!({"path": "sample.zip", "max_bytes": 1024 * 1024});
    let out = run_helper_locally(READ_PROJECT_ARTIFACT_METADATA_HELPER, &payload, tmp.path());
    assert_eq!(out["mime_type"], "application/zip");
    assert_eq!(out["archive_entries_count"], 2);
    assert!(!tmp.path().join("a.txt").exists());
    assert!(!tmp.path().join("b.txt").exists());
}

#[test]
fn helper_read_project_artifact_reads_small_png_single_chunk_and_matches_metadata() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let png = [
        0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n', 0, 0, 0, 13, b'I', b'H', b'D', b'R', 0,
        0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1f, 0x15, 0xc4, 0x89,
    ];
    std::fs::write(tmp.path().join("tiny.png"), png).unwrap();
    let metadata_payload = json!({"path": "tiny.png", "max_bytes": 1024});
    let metadata = run_helper_locally(
        READ_PROJECT_ARTIFACT_METADATA_HELPER,
        &metadata_payload,
        tmp.path(),
    );
    let payload = json!({"path": "tiny.png", "offset": 0, "length": 1024});
    let out = run_helper_locally(READ_PROJECT_ARTIFACT_HELPER, &payload, tmp.path());
    assert_eq!(out["mime_type"], "image/png");
    assert_eq!(out["file_bytes"], png.len());
    assert_eq!(out["sha256"], metadata["sha256"]);
    assert_eq!(out["offset"], 0);
    assert_eq!(out["bytes_returned"], png.len());
    assert_eq!(out["next_offset"], png.len());
    assert_eq!(out["truncated"], false);
    assert_eq!(
        out["content_base64"],
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png)
    );
}

#[test]
fn helper_read_project_artifact_reads_multiple_chunks() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let bytes = b"abcdefghijkl";
    std::fs::write(tmp.path().join("data.bin"), bytes).unwrap();

    let first_payload = json!({"path": "data.bin", "offset": 0, "length": 5});
    let first = run_helper_locally(READ_PROJECT_ARTIFACT_HELPER, &first_payload, tmp.path());
    assert_eq!(first["file_bytes"], bytes.len());
    assert_eq!(first["offset"], 0);
    assert_eq!(first["bytes_returned"], 5);
    assert_eq!(first["next_offset"], 5);
    assert_eq!(first["truncated"], true);
    assert_eq!(
        first["content_base64"],
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes[..5])
    );

    let second_payload = json!({"path": "data.bin", "offset": 5, "length": 20});
    let second = run_helper_locally(READ_PROJECT_ARTIFACT_HELPER, &second_payload, tmp.path());
    assert_eq!(second["sha256"], first["sha256"]);
    assert_eq!(second["offset"], 5);
    assert_eq!(second["bytes_returned"], bytes.len() - 5);
    assert_eq!(second["next_offset"], bytes.len());
    assert_eq!(second["truncated"], false);
    assert_eq!(
        second["content_base64"],
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes[5..])
    );
}

#[test]
fn helper_read_project_artifact_offset_at_eof_returns_empty_chunk() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("data.bin"), b"abc").unwrap();
    let payload = json!({"path": "data.bin", "offset": 3, "length": 10});
    let out = run_helper_locally(READ_PROJECT_ARTIFACT_HELPER, &payload, tmp.path());
    assert_eq!(out["file_bytes"], 3);
    assert_eq!(out["offset"], 3);
    assert_eq!(out["bytes_returned"], 0);
    assert_eq!(out["content_base64"], "");
    assert_eq!(out["next_offset"], 3);
    assert_eq!(out["truncated"], false);
}

#[test]
fn helper_read_project_artifact_rejects_invalid_offset_and_length() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("data.bin"), b"abc").unwrap();
    let bad_offset = run_helper_locally(
        READ_PROJECT_ARTIFACT_HELPER,
        &json!({"path": "data.bin", "offset": -1, "length": 1}),
        tmp.path(),
    );
    assert!(bad_offset["error"].as_str().unwrap().contains("offset"));
    let bad_length = run_helper_locally(
        READ_PROJECT_ARTIFACT_HELPER,
        &json!({"path": "data.bin", "offset": 0, "length": 0}),
        tmp.path(),
    );
    assert!(bad_length["error"].as_str().unwrap().contains("length"));
}

#[tokio::test]
async fn read_project_artifact_rejects_sensitive_path_before_resolving_project() {
    let out = test_runtime()
        .read_project_artifact(
            "agent:missing:missing".to_string(),
            ".env".to_string(),
            None,
            None,
            None,
            None,
        )
        .await;
    assert!(!out.success);
    assert!(out.error.unwrap().contains("sensitive artifact path"));
}

#[tokio::test]
async fn read_project_artifact_rejects_invalid_length_before_resolving_project() {
    let out = test_runtime()
        .read_project_artifact(
            "agent:missing:missing".to_string(),
            "docs/assets/file.png".to_string(),
            None,
            None,
            Some(crate::tool_runtime::files::MAX_READ_PROJECT_ARTIFACT_LENGTH + 1),
            None,
        )
        .await;
    assert!(!out.success);
    assert!(out.error.unwrap().contains("length too large"));
}

#[test]
fn helper_write_project_file_creates_new_file() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let payload = json!({
        "path": "EDIT_PROBE.txt",
        "content": "line1\nline2\n",
        "overwrite": false,
        "expected_sha256": null,
        "expected_content_prefix": null
    });
    let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["created"], true);
    assert_eq!(out["overwritten"], false);
    assert_eq!(out["bytes_written"], 12);
    assert_eq!(out["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(out["warning"], Value::Null);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
        "line1\nline2\n"
    );
}

#[test]
fn helper_write_project_file_existing_without_overwrite_fails() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("EDIT_PROBE.txt"), "original").unwrap();
    let payload = json!({
        "path": "EDIT_PROBE.txt",
        "content": "new",
        "overwrite": false,
        "expected_sha256": null,
        "expected_content_prefix": null
    });
    let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["created"], false);
    assert!(out["error"].as_str().unwrap().contains("overwrite"));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
        "original"
    );
}

#[test]
fn helper_write_project_file_overwrite_with_matching_sha256_succeeds() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("EDIT_PROBE.txt"), "original").unwrap();
    let sha = sha256_hex("original");
    let payload = json!({
        "path": "EDIT_PROBE.txt",
        "content": "replaced",
        "overwrite": true,
        "expected_sha256": sha,
        "expected_content_prefix": null
    });
    let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["overwritten"], true);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
        "replaced"
    );
}

#[test]
fn helper_write_project_file_overwrite_with_mismatched_sha256_fails() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("EDIT_PROBE.txt"), "original").unwrap();
    let payload = json!({
        "path": "EDIT_PROBE.txt",
        "content": "replaced",
        "overwrite": true,
        "expected_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "expected_content_prefix": null
    });
    let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["created"], false);
    assert!(out["error"].as_str().unwrap().contains("sha256"));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
        "original"
    );
}

#[test]
fn helper_write_project_file_overwrite_with_matching_prefix_succeeds() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("EDIT_PROBE.txt"), "v1 content").unwrap();
    let payload = json!({
        "path": "EDIT_PROBE.txt",
        "content": "v1 replaced",
        "overwrite": true,
        "expected_sha256": null,
        "expected_content_prefix": "v1 "
    });
    let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["overwritten"], true);
    assert_eq!(out["warning"], Value::Null);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
        "v1 replaced"
    );
}

#[test]
fn helper_write_project_file_overwrite_with_mismatched_prefix_fails() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("EDIT_PROBE.txt"), "v2 content").unwrap();
    let payload = json!({
        "path": "EDIT_PROBE.txt",
        "content": "x",
        "overwrite": true,
        "expected_sha256": null,
        "expected_content_prefix": "v1 "
    });
    let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["created"], false);
    assert!(out["error"].as_str().unwrap().contains("prefix"));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
        "v2 content"
    );
}

#[test]
fn helper_write_project_file_overwrite_without_guards_warns() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("EDIT_PROBE.txt"), "original").unwrap();
    let payload = json!({
        "path": "EDIT_PROBE.txt",
        "content": "replaced",
        "overwrite": true,
        "expected_sha256": null,
        "expected_content_prefix": null
    });
    let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["overwritten"], true);
    assert!(
        out["warning"].as_str().unwrap().contains("expected_sha256"),
        "should warn about missing guard: {:?}",
        out["warning"]
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
        "replaced"
    );
}

#[test]
fn helper_write_project_file_rejects_nul_content() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let payload = json!({
        "path": "EDIT_PROBE.txt",
        "content": "a\u{0000}b",
        "overwrite": false,
        "expected_sha256": null,
        "expected_content_prefix": null
    });
    let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["created"], false);
    assert!(out["error"].as_str().unwrap().contains("NUL"));
    assert!(!tmp.path().join("EDIT_PROBE.txt").exists());
}

#[test]
fn apply_text_edits_replace_exact_large_block() {
    let original = "mod foo {\n    fn old_a() {\n        todo!()\n    }\n    fn old_b() {\n        todo!()\n    }\n}\n";
    let old_block =
        "    fn old_a() {\n        todo!()\n    }\n    fn old_b() {\n        todo!()\n    }";
    let new_block =
        "    fn new_a() -> u32 {\n        1\n    }\n    fn new_b() -> u32 {\n        2\n    }";
    let edits = vec![text_edit(
        ApplyTextEditKind::ReplaceExact,
        Some(old_block),
        Some(new_block),
        None,
    )];
    let (updated, out) =
        files::apply_text_edits_to_string(original, "src/foo.rs", &edits, None, false).unwrap();
    assert!(updated.contains("fn new_a() -> u32 {"));
    assert!(updated.contains("fn new_b() -> u32 {"));
    assert!(!updated.contains("old_a"));
    assert!(!updated.contains("old_b"));
    assert_eq!(out["path"], "src/foo.rs");
    assert_eq!(out["applied_count"], 1);
    assert_eq!(out["changed"], true);
    assert_eq!(out["would_change"], true);
    assert_eq!(out["edits"][0]["kind"], "replace_exact");
    assert_eq!(out["changed_paths"][0], "src/foo.rs");
}

#[test]
fn apply_text_edits_multiple_edits_atomic() {
    let original = "alpha\nbeta\ngamma\ndelta\n";
    let edits = vec![
        text_edit(
            ApplyTextEditKind::ReplaceExact,
            Some("beta"),
            Some("BETA"),
            None,
        ),
        text_edit(
            ApplyTextEditKind::ReplaceExact,
            Some("delta"),
            Some("DELTA"),
            None,
        ),
    ];
    let (updated, out) =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap();
    assert_eq!(updated, "alpha\nBETA\ngamma\nDELTA\n");
    assert_eq!(out["applied_count"], 2);
    assert_eq!(out["edits"].as_array().unwrap().len(), 2);
}

#[test]
fn apply_text_edits_rejects_missing_match() {
    let original = "alpha\nbeta\n";
    let edits = vec![text_edit(
        ApplyTextEditKind::ReplaceExact,
        Some("nonexistent"),
        Some("x"),
        None,
    )];
    let err =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap_err();
    assert!(err.contains("not found"));
    assert!(err.contains("No files were modified"));
    // Original is untouched (pure function never mutates input).
    assert_eq!(original, "alpha\nbeta\n");
}

#[test]
fn apply_text_edits_rejects_ambiguous_match() {
    let original = "dup\ndup\n";
    let edits = vec![text_edit(
        ApplyTextEditKind::ReplaceExact,
        Some("dup"),
        Some("x"),
        None,
    )];
    let err =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap_err();
    assert!(err.contains("matched 2 times"));
    assert!(err.contains("ambiguous"));
}

#[test]
fn apply_text_edits_expected_file_sha256_guard() {
    let original = "alpha\nbeta\n";
    let edits = vec![text_edit(
        ApplyTextEditKind::ReplaceExact,
        Some("beta"),
        Some("BETA"),
        None,
    )];
    // Wrong sha → rejected before any edit is applied.
    let err = files::apply_text_edits_to_string(
        original,
        "src/x.rs",
        &edits,
        Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
        false,
    )
    .unwrap_err();
    assert!(err.contains("expected_file_sha256 mismatch"));
    // Correct sha → succeeds.
    let real_sha = files::sha256_hex_bytes(original.as_bytes());
    let (updated, _) =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, Some(&real_sha), false)
            .unwrap();
    assert_eq!(updated, "alpha\nBETA\n");
}

#[test]
fn apply_text_edits_insert_before_after_unique_anchor() {
    let original = "header\nbody\nfooter\n";
    // insert_before unique anchor.
    let edits = vec![text_edit(
        ApplyTextEditKind::InsertBefore,
        None,
        Some("// before body\n"),
        Some("body"),
    )];
    let (updated, out) =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap();
    assert_eq!(updated, "header\n// before body\nbody\nfooter\n");
    assert_eq!(out["edits"][0]["kind"], "insert_before");

    // insert_after unique anchor.
    let edits = vec![text_edit(
        ApplyTextEditKind::InsertAfter,
        None,
        Some("// after body\n"),
        Some("body\n"),
    )];
    let (updated, _) =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap();
    assert_eq!(updated, "header\nbody\n// after body\nfooter\n");

    // Ambiguous anchor → rejected.
    let dup = "tag\ntag\n";
    let edits = vec![text_edit(
        ApplyTextEditKind::InsertBefore,
        None,
        Some("x"),
        Some("tag"),
    )];
    let err = files::apply_text_edits_to_string(dup, "src/x.rs", &edits, None, false).unwrap_err();
    assert!(err.contains("matched 2 times"));
}

#[test]
fn apply_text_edits_delete_exact_removes_block() {
    let original = "keep1\ndelete_me\nkeep2\n";
    let edits = vec![text_edit(
        ApplyTextEditKind::DeleteExact,
        Some("delete_me\n"),
        None,
        None,
    )];
    let (updated, out) =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap();
    assert_eq!(updated, "keep1\nkeep2\n");
    assert_eq!(out["edits"][0]["kind"], "delete_exact");
}

#[test]
fn apply_text_edits_rejects_overlapping_edits() {
    let original = "abcdefghij\n";
    // Two replace_exact ops whose ranges overlap.
    let edits = vec![
        text_edit(
            ApplyTextEditKind::ReplaceExact,
            Some("cde"),
            Some("X"),
            None,
        ),
        text_edit(
            ApplyTextEditKind::ReplaceExact,
            Some("def"),
            Some("Y"),
            None,
        ),
    ];
    let err =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap_err();
    assert!(err.contains("overlap"));
}

#[tokio::test]
async fn apply_text_edits_dry_run_does_not_write() {
    let runtime = runtime_with_agent_project("ate-dry");
    register_agent(
        &runtime,
        "ate-dry",
        None,
        ShellClientCapabilities {
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("ate-dry");

    let runtime_for_task = runtime.clone();
    let project_for_task = project.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .apply_text_edits(
                project_for_task,
                "EDIT_PROBE.txt".to_string(),
                vec![text_edit(
                    ApplyTextEditKind::ReplaceExact,
                    Some("old"),
                    Some("new"),
                    None,
                )],
                Some(true),
                None,
            )
            .await
    });

    let mut req = None;
    for _ in 0..20 {
        req = runtime
            .shell_clients
            .poll(ShellAgentPollRequest {
                client_id: "ate-dry".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap();
        if req.is_some() {
            break;
        }
        tokio::task::yield_now().await;
    }
    let req = req.expect("apply_text_edits should enqueue an agent file op");
    assert_eq!(req.kind, "file_apply_text_edits");
    // The payload carries dry_run and the edits.
    let payload: Value = serde_json::from_str(req.content.as_deref().unwrap()).unwrap();
    assert_eq!(payload["dry_run"], true);
    assert_eq!(payload["edits"][0]["kind"], "replace_exact");

    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "ate-dry".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: req.request_id,
            exit_code: Some(0),
            stdout: Some(
                "{\"path\":\"EDIT_PROBE.txt\",\"dry_run\":true,\"applied_count\":1,\
                     \"old_sha256\":\"b\",\"new_sha256\":\"a\",\"changed\":false,\
                     \"would_change\":true,\"edits\":[],\"changed_paths\":[\"EDIT_PROBE.txt\"]}"
                    .to_string(),
            ),
            stderr: Some(String::new()),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["dry_run"], true);
    assert_eq!(result.output["would_change"], true);
    assert_eq!(result.output["changed"], false);
}

#[tokio::test]
async fn apply_text_edits_read_only_session_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_project(tmp.path(), "demo");
    let session = runtime.sessions.start_session_with_guards(
        Some("demo".to_string()),
        Some("read only".to_string()),
        SessionMode::ReadOnly,
        sessions::SessionGuards::default(),
    );

    let result = runtime
        .dispatch(ToolCall::ApplyTextEdits {
            project: "demo".to_string(),
            path: "should-not-exist.txt".to_string(),
            edits: vec![text_edit(
                ApplyTextEditKind::ReplaceExact,
                Some("old"),
                Some("new"),
                None,
            )],
            dry_run: None,
            expected_file_sha256: None,
            session_id: Some(session.session_id.clone()),
        })
        .await;

    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "session_guard_denied");
    assert_eq!(result.output["guard"], "deny_write_tools");
    assert_eq!(result.output["mode"], "read_only");
    assert!(!tmp.path().join("should-not-exist.txt").exists());
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.failed, 1);
    assert_eq!(summary.counts.write_like, 1);
    let event = finished_event(&summary, "apply_text_edits");
    assert_eq!(event.status.as_deref(), Some("failed"));
    assert_eq!(event.error_kind.as_deref(), Some("session_guard_denied"));
}

#[tokio::test]
async fn apply_text_edits_session_event_summary() {
    let runtime = runtime_with_agent_project("ate-sess");
    register_agent(
        &runtime,
        "ate-sess",
        None,
        ShellClientCapabilities {
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("ate-sess");
    let session = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        Some("apply_text_edits session".to_string()),
        SessionMode::Normal,
        sessions::SessionGuards::default(),
    );

    let bootstrap = auth_context(None, true);
    let runtime_for_task = runtime.clone();
    let project_for_task = project.clone();
    let session_id = session.session_id.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .dispatch_with_auth(
                ToolCall::ApplyTextEdits {
                    project: project_for_task,
                    path: "src/lib.rs".to_string(),
                    edits: vec![text_edit(
                        ApplyTextEditKind::ReplaceExact,
                        Some("SECRET_OLD_BLOCK"),
                        Some("SECRET_NEW_BLOCK"),
                        None,
                    )],
                    dry_run: None,
                    expected_file_sha256: None,
                    session_id: Some(session_id),
                },
                Some(&bootstrap),
            )
            .await
    });

    let mut req = None;
    for _ in 0..20 {
        req = runtime
            .shell_clients
            .poll(ShellAgentPollRequest {
                client_id: "ate-sess".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap();
        if req.is_some() {
            break;
        }
        tokio::task::yield_now().await;
    }
    let req = req.expect("apply_text_edits should enqueue an agent file op");
    assert_eq!(req.kind, "file_apply_text_edits");
    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "ate-sess".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: req.request_id,
            exit_code: Some(0),
            stdout: Some(
                "{\"path\":\"src/lib.rs\",\"dry_run\":false,\"applied_count\":1,\
                     \"old_sha256\":\"b\",\"new_sha256\":\"a\",\"changed\":true,\
                     \"would_change\":true,\"edits\":[{\"index\":0,\"kind\":\"replace_exact\",\
                     \"old_start_line\":1,\"old_end_line\":1,\"new_line_count\":1}],\
                     \"changed_paths\":[\"src/lib.rs\"]}"
                    .to_string(),
            ),
            stderr: Some(String::new()),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["changed"], true);
    assert_eq!(result.output["changed_paths"][0], "src/lib.rs");

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.succeeded, 1);
    assert_eq!(summary.counts.write_like, 1);
    let event = finished_event(&summary, "apply_text_edits");
    assert_eq!(event.status.as_deref(), Some("succeeded"));
    // changed_paths recorded from the input path.
    assert!(event.changed_paths.iter().any(|p| p == "src/lib.rs"));
    // input_summary lives on the tool_call_started event; it must NOT leak
    // old_text/new_text contents.
    let started = summary
        .events
        .iter()
        .rev()
        .find(|e| e.kind == "tool_call_started" && e.tool_name == "apply_text_edits")
        .expect("started event for apply_text_edits");
    let input_summary = started
        .input_summary
        .as_ref()
        .expect("input_summary present on started event");
    let summary_str = serde_json::to_string(input_summary).unwrap();
    assert!(summary_str.contains("edit_count"));
    assert!(summary_str.contains("src/lib.rs"));
    assert!(
        !summary_str.contains("SECRET_OLD_BLOCK"),
        "input_summary must not leak old_text content: {}",
        summary_str
    );
    assert!(
        !summary_str.contains("SECRET_NEW_BLOCK"),
        "input_summary must not leak new_text content: {}",
        summary_str
    );
    assert_eq!(input_summary["old_text_present"], true);
    assert_eq!(input_summary["new_text_present"], true);
}

//! Files tests for tool_runtime.

use super::super::files::*;
use super::super::helpers::*;
use super::super::patch::*;
use super::super::types::*;
use super::support::*;
use crate::shell_protocol::{
    ShellAgentPollRequest, ShellAgentResultRequest, ShellClientCapabilities,
};
use serde_json::json;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[tokio::test]
async fn write_project_file_with_session_id_records_changed_path_without_content() {
    let runtime = runtime_with_agent_project("telemetry-write");
    let mut caps = ShellClientCapabilities::default();
    caps.file_write = true;
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
        .expect("write_project_file should enqueue a native file-op request");
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
async fn artifact_upload_chunk_session_log_arguments_do_not_store_base64() {
    let runtime = runtime_with_agent_project("telemetry-artifact-chunk");
    register_agent(
        &runtime,
        "telemetry-artifact-chunk",
        None,
        ShellClientCapabilities {
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("telemetry-artifact-chunk");
    let session = runtime.sessions.start_session(None, None);
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

    let req = next_patch_agent_request(&runtime, "telemetry-artifact-chunk")
        .await
        .expect("artifact_upload_chunk should enqueue a native file-op request");
    let payload: serde_json::Value =
        serde_json::from_str(req.content.as_deref().expect("file-op payload")).unwrap();
    assert_eq!(payload["content_base64"], content_base64);
    complete_patch_agent_request(
        &runtime,
        "telemetry-artifact-chunk",
        &req.request_id,
        0,
        r#"{"path":"artifacts/imports/chunk.txt","upload_id":"wc_upload_test_1","received_bytes":12,"next_offset":12,"expected_bytes":null,"expected_sha256":null,"max_bytes":10485760,"mime_type":null,"committed":false}"#,
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
            event.kind == "tool_call_started" && event.tool_name == "artifact_upload_chunk"
        })
        .expect("started event for artifact_upload_chunk");
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
    assert_eq!(matches[0]["context_before"], json!([]));
    assert_eq!(matches[0]["context_after"], json!([]));
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
    assert!(cmd.contains("command -v rg"));
    assert!(cmd.contains("\"backend\":\"rg\""));
    assert!(cmd.contains("\"backend\":\"grep\""));
    assert!(cmd.contains("rg --with-filename"));
    assert!(cmd.contains("--glob '!**/.git/**'"));
    assert!(cmd.contains("--glob '!**/target/**'"));
    assert!(cmd.contains("--glob '!**/node_modules/**'"));
    assert!(cmd.contains("--exclude-dir=.git"));
    assert!(cmd.contains("--exclude-dir=target"));
    assert!(cmd.contains("--exclude-dir=node_modules"));
    assert!(cmd.contains("--exclude-dir=secrets"));
    assert!(cmd.contains("--exclude=.env"));
    assert!(cmd.contains("--exclude=*.key"));
    assert!(cmd.contains("head -n 26"));
    assert!(cmd.contains("grep -rnI"));
}

#[cfg(unix)]
fn write_executable_script(path: &std::path::Path, body: &str) {
    std::fs::write(path, body).unwrap();
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

#[cfg(unix)]
fn fake_head_script() -> &'static str {
    "#!/bin/sh\nwhile IFS= read -r line; do\n  printf '%s\\n' \"$line\"\ndone\n"
}

#[cfg(unix)]
#[test]
fn search_project_text_command_prefers_rg_backend_when_available() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    write_executable_script(
        &bin.join("rg"),
        "#!/bin/sh\nprintf 'src/lib.rs:2:needle from rg\\n'\n",
    );
    write_executable_script(&bin.join("head"), fake_head_script());

    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command("needle", ".", 5)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 0, "stderr: {stderr}");
    let (matches, truncated, backend) = parse_search_project_text_output(&stdout, 5, 0, 0, false);

    assert_eq!(backend, "rg");
    assert!(!truncated);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["path"], "src/lib.rs");
    assert_eq!(matches[0]["line"], 2);
    assert_eq!(matches[0]["preview"], "needle from rg");
}

#[cfg(unix)]
#[test]
fn search_project_text_command_falls_back_to_grep_without_rg() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    write_executable_script(
        &bin.join("grep"),
        "#!/bin/sh\nprintf 'src/lib.rs:3:needle from grep\\n'\n",
    );
    write_executable_script(&bin.join("head"), fake_head_script());

    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command("needle", ".", 5)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 0, "stderr: {stderr}");
    let (matches, truncated, backend) = parse_search_project_text_output(&stdout, 5, 0, 0, false);

    assert_eq!(backend, "grep");
    assert!(!truncated);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["path"], "src/lib.rs");
    assert_eq!(matches[0]["line"], 3);
    assert_eq!(matches[0]["preview"], "needle from grep");
}

#[test]
fn parse_search_project_text_output_reports_backend_and_limit_truncation() {
    let stdout = concat!(
        "{\"backend\":\"rg\"}\n",
        "src/a.rs:1:needle one\n",
        "src/b.rs:2:needle two\n",
        "{\"backend\":\"rg\"}\n",
    );
    let (matches, truncated, backend) = parse_search_project_text_output(stdout, 1, 0, 0, false);

    assert_eq!(backend, "rg");
    assert!(truncated);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["path"], "src/a.rs");
}

#[tokio::test]
async fn search_project_text_no_matches_returns_empty_matches() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("lib.rs"), "pub fn present() {}\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "search-empty", "demo", tmp.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::SearchProjectText {
                        project,
                        pattern: "absent_needle".to_string(),
                        session_id: None,
                        path: None,
                        limit: Some(5),
                        context_before: None,
                        context_after: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "search-empty")
        .await
        .expect("search_project_text should enqueue an agent search request");
    complete_agent_request_by_running_locally(&runtime, "search-empty", req).await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert!(matches!(
        result.output["backend"].as_str(),
        Some("rg" | "grep")
    ));
    assert_eq!(result.output["matches"], json!([]));
    assert_eq!(result.output["count"], 0);
    assert_eq!(result.output["truncated"], false);
}

#[tokio::test]
async fn search_project_text_excludes_sensitive_and_build_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::create_dir_all(tmp.path().join("target")).unwrap();
    std::fs::create_dir_all(tmp.path().join("node_modules/pkg")).unwrap();
    std::fs::create_dir_all(tmp.path().join("secrets")).unwrap();
    std::fs::create_dir_all(tmp.path().join("tokens")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), "KEEP_SEARCH_NEEDLE\n").unwrap();
    std::fs::write(tmp.path().join(".env"), "KEEP_SEARCH_NEEDLE\n").unwrap();
    std::fs::write(tmp.path().join("target/out.txt"), "KEEP_SEARCH_NEEDLE\n").unwrap();
    std::fs::write(
        tmp.path().join("node_modules/pkg/index.js"),
        "KEEP_SEARCH_NEEDLE\n",
    )
    .unwrap();
    std::fs::write(tmp.path().join("secrets/key.txt"), "KEEP_SEARCH_NEEDLE\n").unwrap();
    std::fs::write(tmp.path().join("tokens/api.txt"), "KEEP_SEARCH_NEEDLE\n").unwrap();
    std::fs::write(tmp.path().join("id.key"), "KEEP_SEARCH_NEEDLE\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "search-excludes", "demo", tmp.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::SearchProjectText {
                        project,
                        pattern: "KEEP_SEARCH_NEEDLE".to_string(),
                        session_id: None,
                        path: None,
                        limit: Some(10),
                        context_before: None,
                        context_after: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "search-excludes")
        .await
        .expect("search_project_text should enqueue an agent search request");
    complete_agent_request_by_running_locally(&runtime, "search-excludes", req).await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["count"], 1);
    assert_eq!(result.output["matches"][0]["path"], "src/lib.rs");
    assert_eq!(result.output["truncated"], false);
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
async fn search_project_text_context_does_not_enqueue_python_helper() {
    let runtime = test_runtime();
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("notes.txt"),
        "before\nneedle appears here\nafter\n",
    )
    .unwrap();
    let project =
        register_agent_project_at_path(&runtime, "search-native", "demo", tmp.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::SearchProjectText {
                        project,
                        pattern: "needle".to_string(),
                        session_id: None,
                        path: None,
                        limit: Some(5),
                        context_before: Some(1),
                        context_after: Some(1),
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "search-native")
        .await
        .expect("search_project_text should enqueue an agent search request");
    let forbidden = ["python3", "-c"].join(" ");
    assert!(
        !req.command.contains(&forbidden),
        "search context must not enqueue a Python helper: {}",
        req.command
    );
    assert!(req.command.contains("command -v rg"));
    assert!(req.command.contains("rg --with-filename --null"));
    assert!(req.command.contains("grep -rnI --null"));
    complete_agent_request_by_running_locally(&runtime, "search-native", req).await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert!(matches!(
        result.output["backend"].as_str(),
        Some("rg" | "grep")
    ));
    assert_eq!(result.output["context_before"], 1);
    assert_eq!(result.output["context_after"], 1);
    let first = &result.output["matches"][0];
    assert_eq!(first["path"], "notes.txt");
    assert_eq!(first["line"], 2);
    assert_eq!(
        first["context_before"][0],
        json!({"line": 1, "text": "before"})
    );
    assert_eq!(
        first["context_after"][0],
        json!({"line": 3, "text": "after"})
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
async fn replace_in_file_routes_to_owning_agent_file_op() {
    let runtime = runtime_with_agent_project("editor");
    let mut caps = ShellClientCapabilities::default();
    caps.file_write = true;
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
    let req = req.expect("replace_in_file should enqueue a file-op request for the agent");
    assert_eq!(req.kind, "file_replace_in_file");
    assert!(req.command.is_empty());
    assert!(req.stdin.is_none());
    // old/new/path travel in the native file-op content payload.
    let payload = req.content.as_deref().expect("file-op payload");
    assert!(payload.contains("EDIT_PROBE.txt"));
    assert!(payload.contains("foo"));
    assert!(payload.contains("bar"));
    assert!(payload.contains("\"expected_replacements\":1"));
    assert!(payload.contains("\"allow_multiple\":false"));
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

#[tokio::test]
async fn artifact_upload_begin_rejects_invalid_inputs_before_resolving_project() {
    let runtime = test_runtime();
    let missing_project = "agent:missing:missing".to_string();
    let cases = [
        (
            ".env",
            Some(1),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            Some("text/plain"),
            "sensitive artifact path",
        ),
        (
            "artifacts/imports/bad-hash.txt",
            Some(1),
            Some("not-a-sha"),
            Some("text/plain"),
            "expected_sha256 must be a lowercase 64-char hex sha256 digest",
        ),
        (
            "artifacts/imports/too-large.txt",
            Some(MAX_PROJECT_ARTIFACT_BYTES + 1),
            None,
            Some("text/plain"),
            "expected_bytes too large",
        ),
        (
            "artifacts/imports/raw.bin",
            Some(1),
            None,
            Some("application/octet-stream"),
            "application/octet-stream requires a safe artifact extension",
        ),
    ];

    for (path, expected_bytes, expected_sha256, mime_type, expected_error) in cases {
        let out = runtime
            .artifact_upload_begin(
                missing_project.clone(),
                path.to_string(),
                expected_bytes,
                expected_sha256.map(str::to_string),
                mime_type.map(str::to_string),
                Some(false),
            )
            .await;
        assert!(!out.success, "{path}");
        assert!(
            out.error.as_deref().unwrap().contains(expected_error),
            "{path}: {:?}",
            out.error
        );
    }
}

#[tokio::test]
async fn artifact_upload_chunk_rejects_invalid_inputs_before_resolving_project() {
    let runtime = test_runtime();
    let missing_project = "agent:missing:missing".to_string();
    let path = "artifacts/imports/chunk.txt".to_string();

    let invalid_id = runtime
        .artifact_upload_chunk(
            missing_project.clone(),
            path.clone(),
            "bad-upload-id".to_string(),
            0,
            "YQ==".to_string(),
        )
        .await;
    assert!(!invalid_id.success);
    assert!(invalid_id.error.unwrap().contains("upload_id must start"));

    let invalid_base64 = runtime
        .artifact_upload_chunk(
            missing_project.clone(),
            path.clone(),
            "wc_upload_test_1".to_string(),
            0,
            "not valid base64!".to_string(),
        )
        .await;
    assert!(!invalid_base64.success);
    assert!(invalid_base64.error.unwrap().contains("invalid base64"));

    let empty = runtime
        .artifact_upload_chunk(
            missing_project.clone(),
            path.clone(),
            "wc_upload_test_1".to_string(),
            0,
            "".to_string(),
        )
        .await;
    assert!(!empty.success);
    assert!(empty
        .error
        .unwrap()
        .contains("decoded chunk must contain at least 1 byte"));

    let oversized = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        vec![b'x'; MAX_PROJECT_ARTIFACT_UPLOAD_CHUNK_BYTES + 1],
    );
    let oversized = runtime
        .artifact_upload_chunk(
            missing_project,
            path,
            "wc_upload_test_1".to_string(),
            0,
            oversized,
        )
        .await;
    assert!(!oversized.success);
    assert!(oversized.error.unwrap().contains("decoded chunk too large"));
}

#[tokio::test]
async fn artifact_upload_finish_and_abort_reject_invalid_upload_id_before_resolving_project() {
    let runtime = test_runtime();
    let missing_project = "agent:missing:missing".to_string();
    let path = "artifacts/imports/file.txt".to_string();

    let finish = runtime
        .artifact_upload_finish(missing_project.clone(), path.clone(), "bad".to_string())
        .await;
    assert!(!finish.success);
    assert!(finish.error.unwrap().contains("upload_id must start"));

    let abort = runtime
        .artifact_upload_abort(missing_project, path, "bad".to_string())
        .await;
    assert!(!abort.success);
    assert!(abort.error.unwrap().contains("upload_id must start"));
}

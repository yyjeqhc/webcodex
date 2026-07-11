//! Files tests for tool_runtime.

use super::super::files::*;
use super::super::helpers::*;
use super::super::patch::*;
use super::super::*;
use super::support::*;
use crate::shell_protocol::{
    ShellAgentPollRequest, ShellAgentResultRequest, ShellAgentShellRequest, ShellClientCapabilities,
};
use serde_json::{json, Value};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[tokio::test]
async fn write_project_file_with_session_id_records_changed_path_without_content() {
    let runtime = runtime_with_agent_project("telemetry-write");
    let mut caps = ShellClientCapabilities::default();
    caps.file_write = true;
    caps.shell = true;
    caps.git = true;
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
    assert_eq!(result.output["permission"]["required"], true);
    assert_eq!(result.output["permission"]["policy"], "dev_auto_approve");
    assert_eq!(result.output["permission"]["status"], "auto_approved");
    assert_eq!(result.output["permission"]["reason"], "dev_auto_approve");
    assert_eq!(result.output["permission"]["risk"], "write");
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.write_like, 1);
    let event = finished_event(&summary, "write_project_file");
    assert!(event.write_like);
    assert_eq!(event.changed_paths, vec!["src/new.txt".to_string()]);
    let permission = event.permission.as_ref().expect("permission metadata");
    assert!(permission.required);
    assert_eq!(permission.policy, "dev_auto_approve");
    assert_eq!(permission.status, "auto_approved");
    assert_eq!(permission.tool_name, "write_project_file");
    assert_eq!(permission.risk, "write");
    let serialized = serde_json::to_string(&summary.events).unwrap();
    assert!(
        !serialized.contains("do-not-log-this-content"),
        "session event leaked write content: {serialized}"
    );

    let handoff = runtime
        .dispatch(ToolCall::SessionHandoffSummary {
            session_id: session.session_id.clone(),
            project: None,
            include_workspace: Some(false),
            include_checkpoints: Some(false),
            include_validation: Some(false),
            summary_only: false,
            limit: None,
        })
        .await;
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(handoff.output["permissions"]["required_count"], 1);
    assert_eq!(handoff.output["permissions"]["auto_approved_count"], 1);
    assert_eq!(handoff.output["permissions"]["manual_approved_count"], 0);
    assert_eq!(handoff.output["permissions"]["approved_count"], 0);
    assert_eq!(handoff.output["permissions"]["total_approved_count"], 1);
    assert_eq!(
        handoff.output["permissions"]["recent"][0]["tool_name"],
        "write_project_file"
    );

    let finish_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::FinishCodingTask {
                        project,
                        session_id,
                        summary_only: false,
                        include_diff: Some(false),
                        include_workspace: None,
                        include_hygiene: Some(false),
                        include_handoff: Some(false),
                        include_validation_summary: Some(false),
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "telemetry-write")
        .await
        .expect("finish_coding_task should inspect changes");
    assert!(req.command.contains("git status --porcelain=v1 -b"));
    complete_patch_agent_request(
        &runtime,
        "telemetry-write",
        &req.request_id,
        0,
        "## main\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nabc123\0abc123\0write\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n",
        "",
    )
    .await;
    let finish = finish_task.await.unwrap();
    assert!(finish.success, "{:?}", finish.error);
    assert_eq!(finish.output["permissions"]["required_count"], 1);
    assert_eq!(finish.output["permissions"]["auto_approved_count"], 1);
    assert_eq!(finish.output["permissions"]["manual_approved_count"], 0);
    assert_eq!(finish.output["permissions"]["approved_count"], 0);
    assert_eq!(finish.output["permissions"]["total_approved_count"], 1);
}

#[tokio::test]
async fn delete_project_files_success_omits_raw_command_output() {
    let runtime = runtime_with_agent_project("cleanup-delete");
    register_agent(
        &runtime,
        "cleanup-delete",
        None,
        ShellClientCapabilities::default(),
    )
    .await;
    let project = agent_test_project_id("cleanup-delete");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .delete_project_files(project, vec!["tmp.txt".to_string()])
                .await
        }
    });

    let req = next_patch_agent_request(&runtime, "cleanup-delete")
        .await
        .expect("delete_project_files should enqueue a shell request");
    assert_eq!(req.kind, "run_shell");
    assert!(req.command.contains("rm -f --"));
    complete_patch_agent_request(
        &runtime,
        "cleanup-delete",
        &req.request_id,
        0,
        "raw stdout should not leak\n",
        "raw stderr should not leak\n",
    )
    .await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["ok"], true);
    assert_eq!(result.output["deleted_paths"], json!(["tmp.txt"]));
    assert_eq!(result.output["missing_paths"], json!([]));
    assert_eq!(result.output["refused_paths"], json!([]));
    assert_eq!(result.output["stdout_present"], true);
    assert_eq!(result.output["stderr_present"], true);
    assert!(result.output.get("command_result").is_none());
    assert!(result.output.get("stdout").is_none());
    assert!(result.output.get("stderr").is_none());
    let serialized = serde_json::to_string(&result.output).unwrap();
    assert!(!serialized.contains("raw stdout should not leak"));
    assert!(!serialized.contains("raw stderr should not leak"));
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
async fn read_project_artifact_metadata_allow_missing_does_not_count_as_failed() {
    let runtime = runtime_with_agent_project("artifact-missing-session");
    register_agent(
        &runtime,
        "artifact-missing-session",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("artifact-missing-session");
    let session = runtime.sessions.start_session(Some(project.clone()), None);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadProjectArtifactMetadata {
                        project,
                        path: "artifacts/smoke/missing.artifact".to_string(),
                        session_id: Some(session_id),
                        allow_missing: Some(true),
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });

    let req = next_patch_agent_request(&runtime, "artifact-missing-session")
        .await
        .expect("read_project_artifact_metadata should enqueue file-op");
    complete_patch_agent_request(
        &runtime,
        "artifact-missing-session",
        &req.request_id,
        0,
        r#"{"path":"artifacts/smoke/missing.artifact","exists":false,"missing":true}"#,
        "",
    )
    .await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["exists"], false);
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.failed, 0);
    let event = finished_event(&summary, "read_project_artifact_metadata");
    assert_eq!(event.status.as_deref(), Some("succeeded"));
}

#[tokio::test]
async fn artifact_upload_begin_policy_rejection_is_classified() {
    let runtime = runtime_with_agent_project("artifact-policy-session");
    register_agent(
        &runtime,
        "artifact-policy-session",
        None,
        ShellClientCapabilities {
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("artifact-policy-session");
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let bootstrap = auth_context(None, true);

    let result = runtime
        .dispatch_with_auth(
            ToolCall::ArtifactUploadBegin {
                project,
                path: "artifacts/smoke/raw.bin".to_string(),
                session_id: Some(session.session_id.clone()),
                expected_bytes: Some(1),
                expected_sha256: None,
                mime_type: Some("application/octet-stream".to_string()),
                overwrite: Some(false),
            },
            Some(&bootstrap),
        )
        .await;

    assert!(!result.success);
    assert!(result.output.get("permission").is_none());
    assert_eq!(result.output["failure_kind"], "policy_rejected");
    assert_eq!(result.output["error_kind"], "policy_rejected");
    let error = result.error.as_deref().unwrap();
    assert!(error.contains(".artifact"), "{error}");
    assert!(error.contains("artifacts/smoke/<name>.artifact"), "{error}");
    assert!(
        next_patch_agent_request(&runtime, "artifact-policy-session")
            .await
            .is_none(),
        "policy rejection must happen before enqueue"
    );

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.failed, 1);
    let event = finished_event(&summary, "artifact_upload_begin");
    assert_eq!(event.failure_kind.as_deref(), Some("policy_rejected"));
    assert_eq!(event.error_kind.as_deref(), Some("policy_rejected"));
    assert!(event.permission.is_none());
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
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(2),
        ..raw_search_request()
    })
    .unwrap();
    let result = search_project_text_output("demo", &options, stdout, Some(0), "");
    let matches = result.output["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 2);
    assert_eq!(result.output["truncated"], true);
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
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(10),
        ..raw_search_request()
    })
    .unwrap();
    let result = search_project_text_output("demo", &options, stdout, Some(0), "");
    let matches = result.output["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["path"], "src/main.rs");
}

#[test]
fn search_project_text_command_excludes_sensitive_dirs_and_bounds_output() {
    let options = SearchOptions::normalize(SearchRequest {
        pattern: "fn main".to_string(),
        path: Some("src".to_string()),
        limit: Some(25),
        context_before: None,
        context_after: None,
        include_globs: None,
        exclude_globs: None,
        result_mode: None,
        timeout_secs: None,
    })
    .unwrap();
    let cmd = search_project_text_command(&options);
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
    assert!(cmd.contains("\"$head_cmd\" -n 26") || cmd.contains("$head_cmd -n 26"));
    assert!(cmd.contains("trap 'cleanup_search_status' EXIT"));
    assert!(cmd.contains("trap 'cleanup_search_status; exit 143' HUP INT TERM"));
    assert!(cmd.contains("grep -rnI"));
    assert!(cmd.contains("command -v head"));
    assert!(cmd.contains("/usr/bin/head") || cmd.contains("/bin/head"));
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

/// Whether the host environment has a working real `rg` (ripgrep) on PATH.
///
/// Used only by integration tests that exercise advanced `search_project_text`
/// features against the installed backend. Fake-`rg` / controlled-PATH tests
/// must not call this — they supply their own backend.
fn host_ripgrep_available() -> bool {
    std::process::Command::new("rg")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
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
        search_project_text_command(
            &SearchOptions::normalize(SearchRequest {
                limit: Some(5),
                ..raw_search_request()
            })
            .unwrap(),
        )
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 0, "stderr: {stderr}");
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), "");

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["backend"], "rg");
    assert_eq!(result.output["truncated"], false);
    assert_eq!(result.output["matches"].as_array().unwrap().len(), 1);
    assert_eq!(result.output["matches"][0]["path"], "src/lib.rs");
    assert_eq!(result.output["matches"][0]["line"], 2);
    assert_eq!(result.output["matches"][0]["preview"], "needle from rg");
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
        search_project_text_command(
            &SearchOptions::normalize(SearchRequest {
                limit: Some(5),
                ..raw_search_request()
            })
            .unwrap(),
        )
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 0, "stderr: {stderr}");
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), "");

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["backend"], "grep");
    assert_eq!(result.output["truncated"], false);
    assert_eq!(result.output["matches"].as_array().unwrap().len(), 1);
    assert_eq!(result.output["matches"][0]["path"], "src/lib.rs");
    assert_eq!(result.output["matches"][0]["line"], 3);
    assert_eq!(result.output["matches"][0]["preview"], "needle from grep");
}

#[test]
fn parse_search_project_text_output_reports_backend_and_limit_truncation() {
    let stdout = concat!(
        "{\"backend\":\"rg\"}\n",
        "src/a.rs:1:needle one\n",
        "src/b.rs:2:needle two\n",
        "{\"backend\":\"rg\"}\n",
    );
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(1),
        ..raw_search_request()
    })
    .unwrap();
    let result = search_project_text_output("demo", &options, stdout, Some(0), "");

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["backend"], "rg");
    assert_eq!(result.output["truncated"], true);
    assert_eq!(result.output["matches"].as_array().unwrap().len(), 1);
    assert_eq!(result.output["matches"][0]["path"], "src/a.rs");
}

fn raw_search_request() -> SearchRequest {
    SearchRequest {
        pattern: "needle".to_string(),
        path: None,
        limit: None,
        context_before: None,
        context_after: None,
        include_globs: None,
        exclude_globs: None,
        result_mode: None,
        timeout_secs: None,
    }
}

fn search_call(project: String, request: SearchRequest) -> ToolCall {
    ToolCall::SearchProjectText {
        project,
        pattern: request.pattern,
        session_id: None,
        path: request.path,
        limit: request.limit,
        context_before: request.context_before,
        context_after: request.context_after,
        include_globs: request.include_globs,
        exclude_globs: request.exclude_globs,
        result_mode: request.result_mode,
        timeout_secs: request.timeout_secs,
    }
}

fn assert_search_output_keys_are_declared(output: &Value) {
    let properties = registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "search_project_text")
        .expect("search_project_text spec")
        .output_schema["properties"]["output"]["properties"]
        .as_object()
        .expect("search_project_text output properties")
        .clone();
    let Some(output) = output.as_object() else {
        return;
    };
    for key in output.keys() {
        assert!(
            properties.contains_key(key),
            "runtime search output key {key} is not declared in output schema"
        );
    }
}

async fn execute_agent_search(
    runtime: &ToolRuntime,
    client_id: &str,
    project: String,
    request: SearchRequest,
) -> (ToolResult, ShellAgentShellRequest) {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(search_call(project, request), Some(&bootstrap))
                .await
        }
    });
    let req = next_patch_agent_request(runtime, client_id)
        .await
        .expect("search_project_text agent request");
    let inspected = req.clone();
    complete_agent_request_by_running_locally(runtime, client_id, req).await;
    let result = task.await.unwrap();
    assert_search_output_keys_are_declared(&result.output);
    (result, inspected)
}

#[test]
fn search_options_default_to_compatible_matches_contract() {
    let options = SearchOptions::normalize(raw_search_request()).unwrap();

    assert_eq!(options.path, ".");
    assert_eq!(options.limit, 50);
    assert_eq!(options.context_before, 0);
    assert_eq!(options.context_after, 0);
    assert_eq!(options.result_mode, SearchResultMode::Matches);
    assert_eq!(options.timeout_secs, 30);
    assert!(options.include_globs.is_empty());
    assert!(options.exclude_globs.is_empty());
    assert!(!options.requires_ripgrep());
}

#[test]
fn search_options_clamp_timeout_and_context() {
    let mut low = raw_search_request();
    low.timeout_secs = Some(0);
    low.context_before = Some(usize::MAX);
    let low = SearchOptions::normalize(low).unwrap();
    assert_eq!(low.timeout_secs, 1);
    assert_eq!(low.context_before, MAX_SEARCH_CONTEXT_LINES);

    let high = SearchOptions::normalize(SearchRequest {
        timeout_secs: Some(999),
        context_after: Some(usize::MAX),
        ..raw_search_request()
    })
    .unwrap();
    assert_eq!(high.timeout_secs, 120);
    assert_eq!(high.context_after, MAX_SEARCH_CONTEXT_LINES);
}

#[test]
fn search_timeout_uses_structured_failure_with_effective_timeout() {
    let options = SearchOptions::normalize(SearchRequest {
        timeout_secs: Some(0),
        ..raw_search_request()
    })
    .unwrap();
    let result = search_project_text_output(
        "demo",
        &options,
        "{\"webcodex_search\":{\"backend\":\"rg\"}}\n",
        Some(-1),
        "Command timed out after 1 seconds",
    );

    assert!(!result.success);
    assert_search_output_keys_are_declared(&result.output);
    assert_eq!(result.output["code"], "search_timeout");
    assert_eq!(result.output["backend"], "rg");
    assert_eq!(result.output["effective_timeout_secs"], 1);
    assert_eq!(result.output["result_mode"], "matches");
}

#[test]
fn search_agent_timeout_budget_keeps_outer_above_command_at_max() {
    // At max configured timeout, shell-client wait is capped at 120 so it may
    // equal command timeout; outer wait must still exceed command timeout.
    let (command, wait, outer) = search_agent_timeout_budget(120);
    assert_eq!(command, 120);
    assert_eq!(wait, 120);
    assert!(outer > command, "outer={outer} command={command}");
    assert!(outer >= wait, "outer={outer} wait={wait}");

    let (command, wait, outer) = search_agent_timeout_budget(30);
    assert_eq!(command, 30);
    assert_eq!(wait, 32);
    assert_eq!(outer, 34);
    assert!(command < wait && wait < outer);

    let (command, wait, outer) = search_agent_timeout_budget(118);
    assert_eq!(command, 118);
    assert_eq!(wait, 120);
    assert!(command < wait);
    assert!(wait < outer || outer > command);
}

#[test]
fn search_backend_exit_1_is_successful_empty_result() {
    let options = SearchOptions::normalize(raw_search_request()).unwrap();
    let stdout = concat!(
        "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n",
        "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n",
    );
    let result = search_project_text_output("demo", &options, stdout, Some(1), "");
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["backend"], "rg");
    assert_eq!(result.output["matches"], json!([]));
    assert_eq!(result.output["count"], 0);
    assert_eq!(result.output["exit_code"], 1);
}

#[test]
fn search_backend_exit_2_is_structured_execution_failure() {
    let options = SearchOptions::normalize(raw_search_request()).unwrap();
    let stdout = concat!(
        "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n",
        "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n",
    );
    let result = search_project_text_output("demo", &options, stdout, Some(2), "");
    assert!(!result.success);
    assert_search_output_keys_are_declared(&result.output);
    assert_eq!(result.output["code"], "search_execution_failed");
    assert_eq!(result.output["backend"], "rg");
    assert_eq!(result.output["result_mode"], "matches");
}

#[test]
fn search_backend_exit_0_parses_matches() {
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let stdout = concat!(
        "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n",
        "src/lib.rs:2:needle from rg\n",
        "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n",
    );
    let result = search_project_text_output("demo", &options, stdout, Some(0), "");
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["backend"], "rg");
    assert_eq!(result.output["matches"].as_array().unwrap().len(), 1);
}

#[test]
fn search_grep_exit_1_is_successful_empty_result() {
    let options = SearchOptions::normalize(raw_search_request()).unwrap();
    let stdout = concat!(
        "{\"webcodex_search\":{\"backend\":\"grep\",\"feature_unavailable\":false}}\n",
        "{\"webcodex_search\":{\"backend\":\"grep\",\"feature_unavailable\":false}}\n",
    );
    let result = search_project_text_output("demo", &options, stdout, Some(1), "");
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["backend"], "grep");
    assert_eq!(result.output["matches"], json!([]));
}

#[test]
fn search_grep_exit_2_is_structured_execution_failure() {
    let options = SearchOptions::normalize(raw_search_request()).unwrap();
    let stdout = concat!(
        "{\"webcodex_search\":{\"backend\":\"grep\",\"feature_unavailable\":false}}\n",
        "{\"webcodex_search\":{\"backend\":\"grep\",\"feature_unavailable\":false}}\n",
    );
    let result = search_project_text_output("demo", &options, stdout, Some(2), "");
    assert!(!result.success);
    assert_eq!(result.output["code"], "search_execution_failed");
    assert_eq!(result.output["backend"], "grep");
}

#[cfg(unix)]
#[test]
fn search_command_preserves_rg_exit_2_despite_head() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    // Fake rg always exits 2 (regex/execution error); head would mask this in a bare pipeline.
    write_executable_script(&bin.join("rg"), "#!/bin/sh\nexit 2\n");
    write_executable_script(&bin.join("head"), fake_head_script());

    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command(&options)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 2, "stderr={stderr} stdout={stdout}");
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(!result.success);
    assert_eq!(result.output["code"], "search_execution_failed");
    assert_eq!(result.output["backend"], "rg");
}

#[cfg(unix)]
#[test]
fn search_command_preserves_rg_exit_1_as_success_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    write_executable_script(&bin.join("rg"), "#!/bin/sh\nexit 1\n");
    write_executable_script(&bin.join("head"), fake_head_script());

    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command(&options)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 1, "stderr={stderr} stdout={stdout}");
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["matches"], json!([]));
    assert_eq!(result.output["backend"], "rg");
}

#[cfg(unix)]
#[test]
fn search_command_preserves_grep_exit_2_despite_head() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    // No rg → grep path.
    write_executable_script(&bin.join("grep"), "#!/bin/sh\nexit 2\n");
    write_executable_script(&bin.join("head"), fake_head_script());

    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command(&options)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 2, "stderr={stderr} stdout={stdout}");
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(!result.success);
    assert_eq!(result.output["code"], "search_execution_failed");
    assert_eq!(result.output["backend"], "grep");
}

#[cfg(unix)]
#[test]
fn search_command_illegal_regex_is_not_swallowed_by_head() {
    // Real rg with an illegal regex should surface exit >= 2 through the generated shell.
    if !host_ripgrep_available() {
        eprintln!("skipping real-ripgrep integration test: rg is unavailable");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.txt"), "hello\n").unwrap();
    let options = SearchOptions::normalize(SearchRequest {
        pattern: "[invalid".to_string(),
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let (exit_code, stdout, stderr, _) =
        run_command_sync(&search_project_text_command(&options), &root, 10);
    assert!(
        exit_code >= 2,
        "illegal regex should fail backend: exit={exit_code} stderr={stderr} stdout={stdout}"
    );
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(!result.success);
    assert_eq!(result.output["code"], "search_execution_failed");
}

#[cfg(unix)]
fn count_webcodex_search_status_files(dir: &std::path::Path) -> usize {
    let mut count = 0usize;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("webcodex-search-") {
            count += 1;
        }
        if entry.path().is_dir() {
            count += count_webcodex_search_status_files(&entry.path());
        }
    }
    count
}

#[cfg(unix)]
#[test]
fn search_status_tmpdir_relative_does_not_use_worktree() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    let rel_tmp = root.join("rel-status-tmp");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&rel_tmp).unwrap();
    write_executable_script(
        &bin.join("rg"),
        "#!/bin/sh\nprintf 'src/a.rs:1:needle\\n'\n",
    );
    write_executable_script(&bin.join("head"), fake_head_script());
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    // Relative TMPDIR must fall back to /tmp — never create status files under project.
    let cmd = format!(
        "PATH={}; export PATH\nTMPDIR=rel-status-tmp; export TMPDIR\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command(&options)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 0, "stderr={stderr} stdout={stdout}");
    assert_eq!(count_webcodex_search_status_files(&root), 0);
    assert_eq!(count_webcodex_search_status_files(&rel_tmp), 0);
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(result.success, "{:?}", result.error);
}

#[cfg(unix)]
#[test]
fn search_status_tmpdir_project_root_does_not_use_worktree() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    let safe_tmp = tmp.path().join("safe-tmp");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&safe_tmp).unwrap();
    write_executable_script(
        &bin.join("rg"),
        "#!/bin/sh\nprintf 'src/a.rs:1:needle\\n'\n",
    );
    write_executable_script(&bin.join("head"), fake_head_script());
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    // Absolute TMPDIR equal to project root is rejected; files must not land in worktree.
    let cmd = format!(
        "PATH={}; export PATH\nTMPDIR={}; export TMPDIR\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        shell_escape_simple(&root.to_string_lossy()),
        search_project_text_command(&options)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 0, "stderr={stderr} stdout={stdout}");
    assert_eq!(count_webcodex_search_status_files(&root), 0);
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(result.success, "{:?}", result.error);
}

#[cfg(unix)]
#[test]
fn search_status_tmpdir_symlink_into_worktree_is_rejected() {
    // Outside symlink → inside worktree dir must not bypass physical-path checks.
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    let inside = root.join("inner-tmp");
    let outside_link = tmp.path().join("outside-link-to-inner");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&inside).unwrap();
    std::os::unix::fs::symlink(&inside, &outside_link).expect("create symlink");
    write_executable_script(
        &bin.join("rg"),
        "#!/bin/sh\nprintf 'src/a.rs:1:needle\\n'\n",
    );
    write_executable_script(&bin.join("head"), fake_head_script());
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let cmd = format!(
        "PATH={}; export PATH\nTMPDIR={}; export TMPDIR\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        shell_escape_simple(&outside_link.to_string_lossy()),
        search_project_text_command(&options)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 0, "stderr={stderr} stdout={stdout}");
    assert_eq!(
        count_webcodex_search_status_files(&root),
        0,
        "symlink-into-worktree TMPDIR must not create status files under the project"
    );
    assert_eq!(count_webcodex_search_status_files(&inside), 0);
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(result.success, "{:?}", result.error);
}

#[cfg(unix)]
#[test]
fn search_status_file_is_removed_after_successful_run() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    let safe_tmp = tmp.path().join("safe-tmp");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&safe_tmp).unwrap();
    write_executable_script(
        &bin.join("rg"),
        "#!/bin/sh\nprintf 'src/a.rs:1:needle\\n'\n",
    );
    write_executable_script(&bin.join("head"), fake_head_script());
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let cmd = format!(
        "PATH={}; export PATH\nTMPDIR={}; export TMPDIR\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        shell_escape_simple(&safe_tmp.to_string_lossy()),
        search_project_text_command(&options)
    );
    let before = count_webcodex_search_status_files(&safe_tmp);
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 0, "stderr={stderr} stdout={stdout}");
    let after = count_webcodex_search_status_files(&safe_tmp);
    assert_eq!(before, 0);
    assert_eq!(after, 0, "status files must be cleaned after success");
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(result.success, "{:?}", result.error);
}

#[cfg(unix)]
#[test]
fn search_status_cleanup_trap_removes_file_on_term() {
    // Mirrors production cleanup_search_status + signal trap; verifies TERM path
    // without a long sleep. File removal is the contract.
    let tmp = tempfile::tempdir().unwrap();
    let status = tmp.path().join("webcodex-search-term-test");
    let script = format!(
        r#"status_file={path}
cleanup_search_status() {{
  if [ -n "${{status_file:-}}" ]; then
    /bin/rm -f "$status_file" 2>/dev/null || /usr/bin/rm -f "$status_file" 2>/dev/null || rm -f "$status_file" 2>/dev/null || true
    status_file=
  fi
}}
trap 'cleanup_search_status' EXIT
trap 'cleanup_search_status; exit 143' HUP INT TERM
: > "$status_file"
kill -s TERM $$
exit 1
"#,
        path = shell_escape_simple(&status.to_string_lossy())
    );
    let (exit_code, _stdout, stderr, _) = run_command_sync(&script, tmp.path(), 5);
    assert!(
        !status.exists(),
        "TERM trap must remove status file; exit={exit_code} stderr={stderr}"
    );
}

#[test]
fn resolve_search_head_command_prefers_path_then_absolute() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    #[cfg(unix)]
    {
        write_executable_script(&bin.join("head"), fake_head_script());
        let path = format!("{}:/usr/bin", bin.display());
        let resolved = resolve_search_head_command(Some(&path), &["/usr/bin/head", "/bin/head"])
            .expect("path head");
        assert!(resolved.contains("bin"), "{resolved}");
        assert!(resolved.ends_with("head"), "{resolved}");
    }
    let missing = resolve_search_head_command(Some("/nonexistent/path/for/head"), &[]);
    assert!(missing.is_none());
    let absolute = resolve_search_head_command(
        Some("/nonexistent/path/for/head"),
        DEFAULT_SEARCH_HEAD_ABSOLUTE_CANDIDATES,
    );
    // System may or may not have /usr/bin/head; when present, absolute resolves.
    if std::path::Path::new("/usr/bin/head").is_file()
        || std::path::Path::new("/bin/head").is_file()
    {
        assert!(absolute.is_some());
    }
}

#[cfg(unix)]
#[test]
fn search_command_fails_when_head_unavailable() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    write_executable_script(
        &bin.join("rg"),
        "#!/bin/sh\nprintf 'src/a.rs:1:needle\\n'\n",
    );
    // No head in PATH and no absolute fallbacks → fail closed (no unbounded output).
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command_with_head_fallbacks(&options, &[])
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 2, "stderr={stderr} stdout={stdout}");
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(!result.success);
    assert_eq!(result.output["code"], "search_execution_failed");
}

#[cfg(unix)]
#[test]
fn search_command_fails_when_head_exits_nonzero_even_if_backend_succeeds() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    write_executable_script(
        &bin.join("rg"),
        "#!/bin/sh\nprintf 'src/a.rs:1:needle\\n'\n",
    );
    write_executable_script(&bin.join("head"), "#!/bin/sh\nexit 2\n");
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    // Prefer PATH head (broken) over absolute system head by using empty absolute fallbacks.
    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command_with_head_fallbacks(&options, &[])
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 2, "stderr={stderr} stdout={stdout}");
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(!result.success);
    assert_eq!(result.output["code"], "search_execution_failed");
}

#[cfg(unix)]
#[test]
fn search_command_keeps_success_when_head_is_available() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    write_executable_script(
        &bin.join("rg"),
        "#!/bin/sh\nprintf 'src/a.rs:1:needle\\n'\n",
    );
    write_executable_script(&bin.join("head"), fake_head_script());
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command(&options)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 0, "stderr={stderr}");
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["matches"].as_array().unwrap().len(), 1);
}

#[test]
fn search_requires_ripgrep_based_on_effective_features_not_field_presence() {
    let empty_globs = SearchOptions::normalize(SearchRequest {
        include_globs: Some(vec![]),
        exclude_globs: Some(vec![]),
        timeout_secs: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    assert!(!empty_globs.requires_ripgrep());
    assert!(empty_globs.include_globs.is_empty());
    assert!(empty_globs.exclude_globs.is_empty());
    assert_eq!(empty_globs.timeout_secs, 5);

    let timeout_only = SearchOptions::normalize(SearchRequest {
        timeout_secs: Some(5),
        result_mode: Some(SearchResultMode::Matches),
        ..raw_search_request()
    })
    .unwrap();
    assert!(!timeout_only.requires_ripgrep());

    let with_include = SearchOptions::normalize(SearchRequest {
        include_globs: Some(vec!["**/*.rs".to_string()]),
        ..raw_search_request()
    })
    .unwrap();
    assert!(with_include.requires_ripgrep());

    let count_mode = SearchOptions::normalize(SearchRequest {
        result_mode: Some(SearchResultMode::Count),
        ..raw_search_request()
    })
    .unwrap();
    assert!(count_mode.requires_ripgrep());
}

#[test]
fn search_validation_errors_are_structured_without_raw_secrets() {
    let secret_pattern = "SUPER_SECRET_PATTERN_VALUE_XYZ";
    let secret_glob = "private-secret-name/**/*.rs";

    let empty = SearchOptions::normalize(SearchRequest {
        pattern: "   ".to_string(),
        ..raw_search_request()
    })
    .unwrap_err();
    assert_eq!(empty.field, "pattern");
    assert_eq!(empty.reason, Some("empty"));

    let nul = SearchOptions::normalize(SearchRequest {
        pattern: format!("a{secret_pattern}\0b"),
        ..raw_search_request()
    })
    .unwrap_err();
    assert_eq!(nul.field, "pattern");
    assert!(!nul.message.contains(secret_pattern));

    let path = SearchOptions::normalize(SearchRequest {
        path: Some("../outside".to_string()),
        ..raw_search_request()
    })
    .unwrap_err();
    assert_eq!(path.field, "path");

    let glob = SearchOptions::normalize(SearchRequest {
        include_globs: Some(vec![format!("!{secret_glob}")]),
        ..raw_search_request()
    })
    .unwrap_err();
    assert_eq!(glob.field, "include_globs");
    assert_eq!(glob.reason, Some("negated"));
    assert_eq!(glob.index, Some(0));
    assert!(!glob.message.contains(secret_glob));
}

#[tokio::test]
async fn search_invalid_request_dispatch_returns_structured_error() {
    // Authorization runs before the tool body; register shell capability so
    // normalize validation is reached and returns structured output.
    let runtime = runtime_with_agent_project("search-invalid");
    register_agent(
        &runtime,
        "search-invalid",
        None,
        ShellClientCapabilities {
            shell: true,
            ..Default::default()
        },
    )
    .await;
    let secret_glob = "NEVER_ECHO_THIS_GLOB_VALUE/**";
    let result = runtime
        .dispatch_with_auth(
            search_call(
                agent_test_project_id("search-invalid"),
                SearchRequest {
                    include_globs: Some(vec![format!("!{secret_glob}")]),
                    ..raw_search_request()
                },
            ),
            Some(&auth_context(None, true)),
        )
        .await;

    // Validation fails before any agent search request is enqueued.
    assert!(!result.success);
    assert_search_output_keys_are_declared(&result.output);
    assert_eq!(result.output["code"], "invalid_search_request");
    assert_eq!(result.output["field"], "include_globs");
    assert_eq!(result.output["reason"], "negated");
    let rendered = serde_json::to_string(&result.output).unwrap();
    assert!(!rendered.contains("NEVER_ECHO_THIS_GLOB_VALUE"));
    assert!(!result.error.as_deref().unwrap_or("").contains("NEVER_ECHO"));
}

#[tokio::test]
async fn search_agent_command_timeout_returns_search_timeout() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.rs"), "needle\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "search-cmd-timeout", "demo", tmp.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    search_call(
                        project,
                        SearchRequest {
                            timeout_secs: Some(1),
                            ..raw_search_request()
                        },
                    ),
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "search-cmd-timeout")
        .await
        .expect("search request");
    assert_eq!(req.timeout_secs, 1);
    // Simulate agent-side command timeout response (lowercase message + error field).
    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "search-cmd-timeout".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: req.request_id,
            exit_code: Some(-1),
            stdout: Some(
                "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n"
                    .to_string(),
            ),
            stderr: Some("command timed out after 1 seconds".to_string()),
            duration_ms: Some(1000),
            error: Some("command timed out".to_string()),
        })
        .await
        .unwrap();
    let result = task.await.unwrap();
    assert!(!result.success);
    assert_search_output_keys_are_declared(&result.output);
    assert_eq!(result.output["code"], "search_timeout");
    assert_eq!(result.output["result_mode"], "matches");
    assert_eq!(result.output["effective_timeout_secs"], 1);
    assert_eq!(result.output["backend"], "rg");
}

#[tokio::test]
async fn search_agent_outer_timeout_returns_search_timeout_and_cancels() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.rs"), "needle\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "search-outer-timeout", "demo", tmp.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    search_call(
                        project,
                        SearchRequest {
                            // Outer wait is command+4 seconds; leave request unanswered.
                            timeout_secs: Some(1),
                            ..raw_search_request()
                        },
                    ),
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "search-outer-timeout")
        .await
        .expect("search request");
    let request_id = req.request_id.clone();
    assert_eq!(req.timeout_secs, 1);
    // Do not complete the agent request; outer tokio timeout should fire.
    let result = task.await.unwrap();
    assert!(!result.success);
    assert_search_output_keys_are_declared(&result.output);
    assert_eq!(result.output["code"], "search_timeout");
    assert_eq!(result.output["result_mode"], "matches");
    assert_eq!(result.output["effective_timeout_secs"], 1);
    assert!(
        result.output.get("backend").is_none() || result.output["backend"].is_null(),
        "outer timeout should not invent backend: {}",
        result.output
    );
    // Request should have been cancelled (no longer pending for completion).
    let complete = runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "search-outer-timeout".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id,
            exit_code: Some(0),
            stdout: Some(String::new()),
            stderr: Some(String::new()),
            duration_ms: Some(1),
            error: None,
        })
        .await;
    assert!(
        complete.is_err(),
        "cancelled request should reject late complete: {complete:?}"
    );
}

#[tokio::test]
async fn search_agent_request_dropped_returns_structured_error() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.rs"), "needle\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "search-dropped", "demo", tmp.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    search_call(
                        project,
                        SearchRequest {
                            timeout_secs: Some(30),
                            ..raw_search_request()
                        },
                    ),
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "search-dropped")
        .await
        .expect("search request");
    // Drop the oneshot waiter without completing — agent disconnect / channel drop.
    runtime.shell_clients.cancel_request(&req.request_id).await;
    let result = task.await.unwrap();
    assert!(!result.success);
    assert_search_output_keys_are_declared(&result.output);
    assert_eq!(result.output["code"], "search_request_dropped");
    assert_eq!(result.output["result_mode"], "matches");
    assert_eq!(result.output["effective_timeout_secs"], 30);
    assert_ne!(result.output["code"], "search_timeout");
    assert!(
        result.error.as_deref().unwrap_or("").contains("dropped"),
        "{:?}",
        result.error
    );
}

#[cfg(unix)]
#[tokio::test]
async fn search_timeout_only_without_rg_still_allows_grep_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("project");
    let bin = tmp.path().join("bin");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(root.join("a.rs"), "timeout_fallback_needle\n").unwrap();
    write_executable_script(
        &bin.join("grep"),
        "#!/bin/sh\nprintf 'a.rs:1:timeout_fallback_needle\\n'\n",
    );
    write_executable_script(&bin.join("head"), fake_head_script());

    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "search-timeout-fallback", "demo", &root).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    search_call(
                        project,
                        SearchRequest {
                            pattern: "timeout_fallback_needle".to_string(),
                            result_mode: Some(SearchResultMode::Matches),
                            timeout_secs: Some(5),
                            ..raw_search_request()
                        },
                    ),
                    Some(&bootstrap),
                )
                .await
        }
    });
    let mut req = next_patch_agent_request(&runtime, "search-timeout-fallback")
        .await
        .expect("search request");
    // Force grep path (no rg in PATH).
    req.command = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        req.command
    );
    assert_eq!(req.timeout_secs, 5);
    complete_agent_request_by_running_locally(&runtime, "search-timeout-fallback", req).await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["backend"], "grep");
    assert_eq!(result.output["effective_timeout_secs"], 5);
    assert_eq!(result.output["matches"].as_array().unwrap().len(), 1);
}

#[test]
fn search_glob_validation_rejects_invalid_and_protected_inputs() {
    let invalid = [
        "",
        "!**/*.rs",
        "docs/\n*.md",
        "docs/\t*.md",
        "docs/\0*.md",
        "**/.env",
        "secrets/**",
        "**/*.key",
    ];
    for glob in invalid {
        let result = SearchOptions::normalize(SearchRequest {
            include_globs: Some(vec![glob.to_string()]),
            ..raw_search_request()
        });
        assert!(result.is_err(), "include glob {glob:?} should be rejected");
        let err = result.unwrap_err();
        assert_eq!(err.field, "include_globs");
        assert!(!err.message.contains(glob) || glob.is_empty());
    }

    for glob in ["", "!vendor/**", "vendor/\n**"] {
        let result = SearchOptions::normalize(SearchRequest {
            exclude_globs: Some(vec![glob.to_string()]),
            ..raw_search_request()
        });
        assert!(result.is_err(), "exclude glob {glob:?} should be rejected");
        assert_eq!(result.unwrap_err().field, "exclude_globs");
    }
}

#[test]
fn search_glob_validation_enforces_count_and_byte_limits() {
    let too_many = (0..=MAX_SEARCH_GLOBS)
        .map(|index| format!("src/{index}/**"))
        .collect::<Vec<_>>();
    let count_error = SearchOptions::normalize(SearchRequest {
        include_globs: Some(too_many),
        ..raw_search_request()
    })
    .unwrap_err();
    assert_eq!(count_error.field, "include_globs");
    assert_eq!(count_error.reason, Some("too_many"));
    assert!(
        count_error.message.contains("at most 32"),
        "{}",
        count_error.message
    );

    let length_error = SearchOptions::normalize(SearchRequest {
        exclude_globs: Some(vec!["a".repeat(MAX_SEARCH_GLOB_BYTES + 1)]),
        ..raw_search_request()
    })
    .unwrap_err();
    assert_eq!(length_error.field, "exclude_globs");
    assert_eq!(length_error.reason, Some("too_long"));
    assert_eq!(length_error.index, Some(0));
    assert!(
        length_error.message.contains("256 bytes"),
        "{}",
        length_error.message
    );
}

#[test]
fn search_audit_arguments_record_bounded_feature_summary_without_pattern_or_globs() {
    let raw = json!({
        "project": "agent:demo:project",
        "pattern": "NEVER_LOG_PATTERN_VALUE",
        "path": "src",
        "limit": 7,
        "context_before": 1,
        "context_after": 2,
        "include_globs": ["private name/**/*.rs"],
        "exclude_globs": ["generated secret name/**"],
        "result_mode": "count",
        "timeout_secs": 45
    });
    let raw_summary = super::super::tool_audit::session_log_arguments_for_tool_request(
        "search_project_text",
        &raw,
    );
    assert_eq!(raw_summary["pattern_present"], true);
    assert_eq!(raw_summary["include_glob_count"], 1);
    assert_eq!(raw_summary["exclude_glob_count"], 1);
    assert_eq!(raw_summary["result_mode"], "count");
    assert_eq!(raw_summary["timeout_secs"], 45);
    let raw_json = serde_json::to_string(&raw_summary).unwrap();
    assert!(!raw_json.contains("NEVER_LOG_PATTERN_VALUE"));
    assert!(!raw_json.contains("private name"));
    assert!(!raw_json.contains("generated secret name"));

    let call_summary = search_call(
        "agent:demo:project".to_string(),
        SearchRequest {
            pattern: "NEVER_LOG_PATTERN_VALUE".to_string(),
            include_globs: Some(vec!["private name/**/*.rs".to_string()]),
            exclude_globs: Some(vec!["generated secret name/**".to_string()]),
            result_mode: Some(SearchResultMode::Count),
            timeout_secs: Some(45),
            ..raw_search_request()
        },
    )
    .session_log_arguments();
    assert_eq!(call_summary["include_glob_count"], 1);
    assert_eq!(call_summary["exclude_glob_count"], 1);
    assert_eq!(call_summary["result_mode"], "count");
    assert_eq!(call_summary["timeout_secs"], 45);
    let call_json = serde_json::to_string(&call_summary).unwrap();
    assert!(!call_json.contains("NEVER_LOG_PATTERN_VALUE"));
    assert!(!call_json.contains("private name"));
    assert!(!call_json.contains("generated secret name"));
}

#[cfg(unix)]
#[test]
fn search_command_passes_shell_metacharacter_globs_as_one_literal_argument() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    write_executable_script(
        &bin.join("rg"),
        "#!/bin/sh\nfor arg do printf 'ARG=%s\\n' \"$arg\"; done\n",
    );
    write_executable_script(&bin.join("head"), fake_head_script());
    let literal = "src/**/space $HOME; 'double\" `tick`";
    let options = SearchOptions::normalize(SearchRequest {
        include_globs: Some(vec![literal.to_string()]),
        ..raw_search_request()
    })
    .unwrap();
    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command(&options)
    );

    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 0, "stderr: {stderr}");
    assert!(stdout.contains(&format!("ARG={literal}")), "{stdout}");
    assert!(
        stdout.contains("$HOME"),
        "environment expansion leaked into argv: {stdout}"
    );
    assert!(
        !stdout.contains("\ndouble\" `tick`\n"),
        "glob split into a command: {stdout}"
    );
}

#[tokio::test]
async fn search_project_text_include_and_exclude_globs_are_additive() {
    // include/exclude globs are ripgrep-only; without host rg this is a
    // capability error, not a product regression (see
    // advanced_search_without_rg_returns_structured_capability_error).
    if !host_ripgrep_available() {
        eprintln!("skipping real-ripgrep integration test: rg is unavailable");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::create_dir_all(tmp.path().join("docs")).unwrap();
    std::fs::create_dir_all(tmp.path().join("vendor")).unwrap();
    std::fs::create_dir_all(tmp.path().join("secrets")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), "SCOPE_NEEDLE\n").unwrap();
    std::fs::write(tmp.path().join("docs/guide.md"), "SCOPE_NEEDLE\n").unwrap();
    std::fs::write(tmp.path().join("vendor/generated.rs"), "SCOPE_NEEDLE\n").unwrap();
    std::fs::write(tmp.path().join("secrets/hidden.rs"), "SCOPE_NEEDLE\n").unwrap();
    std::fs::write(tmp.path().join("notes.txt"), "SCOPE_NEEDLE\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "search-globs", "demo", tmp.path()).await;

    let (result, _) = execute_agent_search(
        &runtime,
        "search-globs",
        project,
        SearchRequest {
            pattern: "SCOPE_NEEDLE".to_string(),
            include_globs: Some(vec!["**/*.rs".to_string(), "docs/**/*.md".to_string()]),
            exclude_globs: Some(vec!["vendor/**".to_string()]),
            limit: Some(10),
            ..raw_search_request()
        },
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    let paths = result.output["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["path"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(paths, vec!["docs/guide.md", "src/lib.rs"]);
    assert_eq!(result.output["result_mode"], "matches");
}

#[tokio::test]
async fn search_project_text_files_with_matches_is_unique_stable_and_bounded() {
    // files_with_matches is ripgrep-only; without host rg this is a capability
    // error, not a product regression (see
    // advanced_search_without_rg_returns_structured_capability_error).
    if !host_ripgrep_available() {
        eprintln!("skipping real-ripgrep integration test: rg is unavailable");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("b.rs"), "FILE_NEEDLE\nFILE_NEEDLE\n").unwrap();
    std::fs::write(tmp.path().join("a.rs"), "FILE_NEEDLE\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "search-files", "demo", tmp.path()).await;

    let (result, _) = execute_agent_search(
        &runtime,
        "search-files",
        project,
        SearchRequest {
            pattern: "FILE_NEEDLE".to_string(),
            limit: Some(1),
            result_mode: Some(SearchResultMode::FilesWithMatches),
            ..raw_search_request()
        },
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["result_mode"], "files_with_matches");
    assert_eq!(result.output["files"], json!([{"path": "a.rs"}]));
    assert_eq!(result.output["returned_file_count"], 1);
    assert_eq!(result.output["truncated"], true);
    assert_eq!(result.output["truncation_reason"], "limit");
}

#[tokio::test]
async fn search_project_text_count_distinguishes_complete_and_truncated_totals() {
    // count result mode is ripgrep-only; without host rg this is a capability
    // error, not a product regression (see
    // advanced_search_without_rg_returns_structured_capability_error).
    if !host_ripgrep_available() {
        eprintln!("skipping real-ripgrep integration test: rg is unavailable");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.rs"), "COUNT_NEEDLE\nCOUNT_NEEDLE\n").unwrap();
    std::fs::write(tmp.path().join("b.rs"), "COUNT_NEEDLE\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "search-count", "demo", tmp.path()).await;

    let (truncated, _) = execute_agent_search(
        &runtime,
        "search-count",
        project.clone(),
        SearchRequest {
            pattern: "COUNT_NEEDLE".to_string(),
            limit: Some(1),
            result_mode: Some(SearchResultMode::Count),
            ..raw_search_request()
        },
    )
    .await;
    assert!(truncated.success, "{:?}", truncated.error);
    assert_eq!(
        truncated.output["files"],
        json!([{"path": "a.rs", "match_count": 2}])
    );
    assert_eq!(truncated.output["returned_file_count"], 1);
    assert_eq!(truncated.output["returned_match_count"], 2);
    assert_eq!(truncated.output["count_complete"], false);
    assert_eq!(truncated.output["total_matches"], Value::Null);
    assert_eq!(truncated.output["truncated"], true);

    let (complete, _) = execute_agent_search(
        &runtime,
        "search-count",
        project,
        SearchRequest {
            pattern: "COUNT_NEEDLE".to_string(),
            limit: Some(10),
            result_mode: Some(SearchResultMode::Count),
            ..raw_search_request()
        },
    )
    .await;
    assert!(complete.success, "{:?}", complete.error);
    assert_eq!(complete.output["returned_file_count"], 2);
    assert_eq!(complete.output["returned_match_count"], 3);
    assert_eq!(complete.output["count_complete"], true);
    assert_eq!(complete.output["total_matches"], 3);
    assert_eq!(complete.output["truncated"], false);
}

#[tokio::test]
async fn search_project_text_reports_effective_clamped_timeout() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.rs"), "TIMEOUT_NEEDLE\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "search-timeout", "demo", tmp.path()).await;

    let (low, low_req) = execute_agent_search(
        &runtime,
        "search-timeout",
        project.clone(),
        SearchRequest {
            pattern: "TIMEOUT_NEEDLE".to_string(),
            timeout_secs: Some(0),
            ..raw_search_request()
        },
    )
    .await;
    assert!(low.success, "{:?}", low.error);
    assert_eq!(low_req.timeout_secs, 1);
    assert_eq!(low.output["effective_timeout_secs"], 1);

    let (high, high_req) = execute_agent_search(
        &runtime,
        "search-timeout",
        project,
        SearchRequest {
            pattern: "TIMEOUT_NEEDLE".to_string(),
            timeout_secs: Some(999),
            ..raw_search_request()
        },
    )
    .await;
    assert!(high.success, "{:?}", high.error);
    assert_eq!(high_req.timeout_secs, 120);
    assert_eq!(high.output["effective_timeout_secs"], 120);
}

#[tokio::test]
async fn advanced_search_without_rg_returns_structured_capability_error() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("project");
    let bin = tmp.path().join("bin");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(root.join("a.rs"), "needle\n").unwrap();
    let runtime = test_runtime();
    let project = register_agent_project_at_path(&runtime, "search-no-rg", "demo", &root).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    search_call(
                        project,
                        SearchRequest {
                            result_mode: Some(SearchResultMode::Count),
                            ..raw_search_request()
                        },
                    ),
                    Some(&bootstrap),
                )
                .await
        }
    });
    let mut req = next_patch_agent_request(&runtime, "search-no-rg")
        .await
        .expect("advanced search agent request");
    req.command = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        req.command
    );
    complete_agent_request_by_running_locally(&runtime, "search-no-rg", req).await;
    let result = task.await.unwrap();

    assert!(!result.success);
    assert_search_output_keys_are_declared(&result.output);
    assert_eq!(result.output["code"], "search_backend_feature_unavailable");
    assert_eq!(result.output["backend"], "grep");
    assert_eq!(
        result.output["requested_features"],
        json!(["result_mode=count"])
    );
    assert!(result.error.unwrap().contains("ripgrep"));
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
                        include_globs: None,
                        exclude_globs: None,
                        result_mode: None,
                        timeout_secs: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "search-empty")
        .await
        .expect("search_project_text should enqueue an agent search request");
    assert_eq!(req.timeout_secs, 30);
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
    assert_eq!(result.output["result_mode"], "matches");
    assert_eq!(result.output["effective_timeout_secs"], 30);
    assert_eq!(result.output["truncation_reason"], Value::Null);
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
                        include_globs: None,
                        exclude_globs: None,
                        result_mode: None,
                        timeout_secs: None,
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
async fn project_overview_requires_file_read_capability() {
    let runtime = runtime_with_agent_project("overview-capability");
    register_agent(
        &runtime,
        "overview-capability",
        None,
        ShellClientCapabilities::default(),
    )
    .await;
    let bootstrap = auth_context(None, true);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ProjectOverview {
                project: agent_test_project_id("overview-capability"),
                session_id: None,
                path: None,
                max_depth: None,
                limit: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("file_read"));
}

#[tokio::test]
async fn project_overview_routes_to_owning_agent_and_returns_structured_metadata() {
    let temp = tempfile::tempdir().unwrap();
    for path in [
        "AGENTS.md",
        "README.md",
        "Cargo.toml",
        "src/lib.rs",
        "target/debug/output",
        ".env",
    ] {
        let path = temp.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "private fixture content").unwrap();
    }
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "overview-agent", "demo", temp.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::ProjectOverview {
                        project,
                        session_id: None,
                        path: None,
                        max_depth: Some(99),
                        limit: Some(1),
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let request = next_patch_agent_request(&runtime, "overview-agent")
        .await
        .expect("project_overview owning-agent request");
    assert_eq!(request.kind, "file_project_overview");
    assert!(
        request.command.is_empty(),
        "project_overview must not use shell"
    );
    let options: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert_eq!(options["max_depth"], 4);
    assert_eq!(options["limit"], 20);
    complete_project_overview_agent_request_locally(&runtime, "overview-agent", &request).await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["schema_version"], 1);
    assert_eq!(result.output["project"], project);
    assert_eq!(result.output["path"], "");
    assert_eq!(result.output["deterministic"], true);
    assert_eq!(result.output["scan"]["max_depth"], 4);
    assert_eq!(result.output["scan"]["limit"], 20);
    let declared_output = registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "project_overview")
        .expect("project_overview spec")
        .output_schema["properties"]["output"]["properties"]
        .as_object()
        .expect("project_overview output schema")
        .clone();
    for key in result.output.as_object().unwrap().keys() {
        assert!(
            declared_output.contains_key(key),
            "runtime project_overview output key {key} is missing from schema"
        );
    }
    let serialized = result.output.to_string();
    assert!(!serialized.contains("private fixture content"));
    assert!(!serialized.contains("target"));
    assert!(!serialized.contains(".env"));
    assert!(!serialized.contains(&temp.path().display().to_string()));
}

#[tokio::test]
async fn project_overview_rejects_invalid_paths_before_agent_request() {
    let runtime = runtime_with_agent_project("overview-path");
    register_agent(
        &runtime,
        "overview-path",
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
                ToolCall::ProjectOverview {
                    project: agent_test_project_id("overview-path"),
                    session_id: None,
                    path: Some(path.to_string()),
                    max_depth: None,
                    limit: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success, "{path} must be rejected");
        assert!(result.error.unwrap().contains("path"));
    }
    assert!(next_patch_agent_request(&runtime, "overview-path")
        .await
        .is_none());
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
                include_globs: None,
                exclude_globs: None,
                result_mode: None,
                timeout_secs: None,
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
                        include_globs: None,
                        exclude_globs: None,
                        result_mode: None,
                        timeout_secs: None,
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
    assert!(err.contains("agent"), "{err}");
    assert!(!err.contains("projects.toml"), "{err}");
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
                include_globs: None,
                exclude_globs: None,
                result_mode: None,
                timeout_secs: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("pattern"));
    assert_eq!(result.output["code"], "invalid_search_request");
    assert_eq!(result.output["field"], "pattern");
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
                    include_globs: None,
                    exclude_globs: None,
                    result_mode: None,
                    timeout_secs: None,
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
        assert_eq!(result.output["code"], "invalid_search_request");
        assert_eq!(result.output["field"], "path");
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
            "artifacts/smoke/<name>.artifact",
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

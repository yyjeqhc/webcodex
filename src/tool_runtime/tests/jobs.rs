//! Jobs tests for tool_runtime.

use super::super::helpers::*;
use super::super::kernel::{ToolCallContext, ToolCallRequest, ToolTransport};
use super::super::types::*;
use super::super::ToolRuntime;
use super::support::*;
use crate::shell_protocol::{ShellClientCapabilities, ShellClientRegisterRequest};
use serde_json::json;
use std::fs;

#[tokio::test]
async fn run_shell_session_events_record_exit_without_stdio_bodies() {
    let runtime = runtime_with_agent_project("telemetry-shell");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "telemetry-shell", None, caps).await;
    let project = agent_test_project_id("telemetry-shell");
    let session = runtime.sessions.start_session(None, None);

    let ok_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::RunShell {
                        project,
                        command: "printf shell-secret-out; printf shell-secret-err >&2".to_string(),
                        session_id: Some(session_id),
                        timeout_secs: Some(30),
                        cwd: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "telemetry-shell")
        .await
        .expect("run_shell should enqueue success request");
    complete_patch_agent_request(
        &runtime,
        "telemetry-shell",
        &req.request_id,
        0,
        "shell-secret-out",
        "shell-secret-err",
    )
    .await;
    let ok = ok_task.await.unwrap();
    assert!(ok.success, "{:?}", ok.error);
    assert_eq!(ok.output["session_recorded"], true);
    assert_eq!(ok.output["permission"]["required"], true);
    assert_eq!(ok.output["permission"]["policy"], "dev_auto_approve");
    assert_eq!(ok.output["permission"]["status"], "auto_approved");
    assert_eq!(ok.output["permission"]["risk"], "shell");

    let fail_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::RunShell {
                        project,
                        command: "printf fail-secret-out; printf fail-secret-err >&2; exit 7"
                            .to_string(),
                        session_id: Some(session_id),
                        timeout_secs: Some(30),
                        cwd: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "telemetry-shell")
        .await
        .expect("run_shell should enqueue failure request");
    complete_patch_agent_request(
        &runtime,
        "telemetry-shell",
        &req.request_id,
        7,
        "fail-secret-out",
        "fail-secret-err",
    )
    .await;
    let fail = fail_task.await.unwrap();
    assert!(!fail.success);
    assert_eq!(fail.output["failure_kind"], "command_exit_nonzero");
    assert_eq!(fail.output["session_recorded"], true);
    assert_eq!(fail.output["permission"]["status"], "auto_approved");

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.tool_calls, 2);
    assert_eq!(summary.counts.succeeded, 1);
    assert_eq!(summary.counts.failed, 1);
    assert_eq!(summary.counts.shell_like, 2);
    let permission_summary = crate::tool_runtime::permissions::permission_summary_from_events(
        &summary.events,
        crate::tool_runtime::permissions::DEFAULT_PERMISSION_RECENT_LIMIT,
    );
    assert_eq!(permission_summary["required_count"], 2);
    assert_eq!(permission_summary["auto_approved_count"], 2);
    let failed = summary
        .events
        .iter()
        .rev()
        .find(|event| {
            event.kind == "tool_call_finished"
                && event.tool_name == "run_shell"
                && event.status.as_deref() == Some("failed")
        })
        .unwrap();
    assert_eq!(failed.exit_code, Some(7));
    assert_eq!(failed.failure_kind.as_deref(), Some("command_exit_nonzero"));
    assert_eq!(failed.error_kind.as_deref(), Some("command_exit_nonzero"));
    let permission = failed.permission.as_ref().expect("permission metadata");
    assert_eq!(permission.status, "auto_approved");
    assert_eq!(permission.risk, "shell");
    let serialized = serde_json::to_string(&summary.events).unwrap();
    for leaked in [
        "shell-secret-out",
        "shell-secret-err",
        "fail-secret-out",
        "fail-secret-err",
    ] {
        assert!(
            !serialized.contains(leaked),
            "session event leaked shell output {leaked}: {serialized}"
        );
    }
    assert!(serialized.contains("\"command_present\":true"));
}

#[test]
fn is_safe_job_id_rejects_path_traversal_and_separators() {
    assert!(is_safe_job_id("11111111-2222-3333-4444-555555555555"));
    assert!(is_safe_job_id("job.1_2-3"));
    assert!(!is_safe_job_id("../escape"));
    assert!(!is_safe_job_id("a/b"));
    assert!(!is_safe_job_id("a\\b"));
    assert!(!is_safe_job_id(".."));
    assert!(!is_safe_job_id("a..b/../c"));
    assert!(!is_safe_job_id(""));
    assert!(!is_safe_job_id("a\0b"));
}

#[test]
fn normalize_local_status_maps_known_and_unknown_values() {
    assert_eq!(normalize_local_status("running"), "running");
    assert_eq!(normalize_local_status("completed"), "completed");
    assert_eq!(normalize_local_status("failed"), "failed");
    assert_eq!(normalize_local_status("stopped"), "stopped");
    assert_eq!(normalize_local_status("queued"), "queued");
    assert_eq!(normalize_local_status("  failed  "), "failed");
    assert_eq!(normalize_local_status(""), "running");
    assert_eq!(normalize_local_status("weird-state"), "lost");
}

#[test]
fn read_lines_from_is_bounded_by_default() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("stdout.log");
    let content = (1..=1000)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&path, &content).unwrap();
    let (text, next) = read_lines_from(path, None, None);
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines.len() <= MAX_LOCAL_LOG_LINES);
    assert_eq!(lines.len(), MAX_LOCAL_LOG_LINES);
    // Default is tail: last 500 lines.
    assert_eq!(lines[0], "line 501");
    assert_eq!(lines.last().unwrap(), &"line 1000");
    assert_eq!(next, 1001);
}

#[test]
fn read_lines_from_supports_offset_pagination() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("stdout.log");
    let content = (1..=600)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&path, &content).unwrap();
    let (text, next) = read_lines_from(path.clone(), Some(1), None);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), MAX_LOCAL_LOG_LINES);
    assert_eq!(lines[0], "line 1");
    assert_eq!(lines.last().unwrap(), &"line 500");
    assert_eq!(next, 501);

    let (text2, next2) = read_lines_from(path, Some(501), None);
    let lines2: Vec<&str> = text2.lines().collect();
    assert_eq!(lines2.len(), 100);
    assert_eq!(lines2[0], "line 501");
    assert_eq!(lines2.last().unwrap(), &"line 600");
    assert_eq!(next2, 601);
}

#[test]
fn read_lines_from_supports_tail_lines() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("stdout.log");
    let content = (1..=1000)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&path, &content).unwrap();
    let (text, _next) = read_lines_from(path, None, Some(10));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 10);
    assert_eq!(lines[0], "line 991");
    assert_eq!(lines.last().unwrap(), &"line 1000");
}

#[test]
fn read_lines_from_tail_is_capped_to_max() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("stdout.log");
    let content = (1..=1000)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&path, &content).unwrap();
    // Requesting more than MAX returns at most MAX.
    let (text, _) = read_lines_from(path, None, Some(5000));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), MAX_LOCAL_LOG_LINES);
}

#[tokio::test]
async fn recover_local_job_status_after_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let project_id = "demo";
    let runtime = runtime_with_project(root, project_id);
    let job_id = "11111111-2222-3333-4444-555555555555";
    write_fake_job(
        root,
        job_id,
        project_id,
        &root.to_string_lossy(),
        "completed",
        "hello\n",
        "",
        json!({}),
    );
    // local_jobs is empty (simulating restart); recovery should find it.
    assert!(runtime.local_jobs.lock().await.is_empty());
    let result = runtime.job_status(job_id.to_string()).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["status"], "completed");
    assert_eq!(result.output["project"], project_id);
    assert_eq!(result.output["executor"], "local");
    assert_eq!(result.output["kind"], "shell");
    assert!(result.output.get("command_preview").is_none());
    let debug_result = runtime
        .dispatch(ToolCall::JobStatus {
            job_id: job_id.to_string(),
            include_command_preview: true,
        })
        .await;
    assert!(debug_result.success, "{:?}", debug_result.error);
    assert_eq!(debug_result.output["command_preview"], "echo test");
    // Recovered job is now cached in memory.
    assert!(runtime.local_jobs.lock().await.contains_key(job_id));
}

#[tokio::test]
async fn recover_local_job_log_after_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let runtime = runtime_with_project(root, "demo");
    let job_id = "22222222-3333-4444-5555-666666666666";
    write_fake_job(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "running",
        "stdout line\n",
        "stderr line\n",
        json!({}),
    );
    let result = runtime.job_log(job_id.to_string(), None, None).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["stdout"], "stdout line");
    assert_eq!(result.output["stderr"], "stderr line");
    assert!(result.output["next_stdout_line"].is_number());
}

#[tokio::test]
async fn recover_local_job_rejects_unsafe_job_id() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_project(tmp.path(), "demo");
    // Path-traversal job ids must not reach the filesystem.
    let result = runtime.job_status("../escape".to_string()).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("unknown job"));
}

#[tokio::test]
async fn recover_local_job_rejects_metadata_project_mismatch() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let runtime = runtime_with_project(root, "demo");
    let job_id = "33333333-4444-5555-6666-777777777777";
    // Metadata claims project "other"; configured project is "demo".
    write_fake_job(
        root,
        job_id,
        "other",
        &root.to_string_lossy(),
        "running",
        "",
        "",
        json!({}),
    );
    let result = runtime.job_status(job_id.to_string()).await;
    assert!(!result.success, "mismatched metadata must not be recovered");
}

#[tokio::test]
async fn recover_local_job_rejects_metadata_path_mismatch() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let runtime = runtime_with_project(root, "demo");
    let job_id = "44444444-5555-6666-7777-888888888888";
    // Metadata path points elsewhere even though project id matches.
    write_fake_job(
        root,
        job_id,
        "demo",
        "/some/other/path",
        "running",
        "",
        "",
        json!({}),
    );
    let result = runtime.job_status(job_id.to_string()).await;
    assert!(
        !result.success,
        "mismatched metadata path must not be recovered"
    );
}

#[tokio::test]
async fn recover_local_job_unknown_when_no_metadata_anywhere() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_project(tmp.path(), "demo");
    let result = runtime
        .job_status("55555555-6666-7777-8888-999999999999".to_string())
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("unknown job"));
}

#[tokio::test]
async fn run_shell_failure_reports_command_started_and_output_tail() {
    let runtime = runtime_with_agent_project("shell-failer");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "shell-failer", None, caps).await;
    let project = agent_test_project_id("shell-failer");
    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .run_shell(
                project,
                "printf run-shell-out; printf run-shell-err >&2; exit 7".to_string(),
                Some(30),
                None,
            )
            .await
    });
    let req = next_patch_agent_request(&runtime, "shell-failer")
        .await
        .expect("run_shell should enqueue a shell command");
    complete_patch_agent_request(
        &runtime,
        "shell-failer",
        &req.request_id,
        7,
        "run-shell-out",
        "run-shell-err",
    )
    .await;
    let result = task.await.unwrap();
    assert!(!result.success);
    let error = result.error.as_deref().unwrap_or("");
    assert!(error.contains("Command exited with status 7"));
    assert!(error.contains("No files were modified by WebCodex itself"));
    assert!(error.contains("stdout_tail"));
    assert!(error.contains("stderr_tail"));
    assert!(error.contains("Retry guidance"));
    assert_eq!(result.output["exit_code"], 7);
    assert_eq!(result.output["stdout_tail"], "run-shell-out");
    assert_eq!(result.output["stderr_tail"], "run-shell-err");
    assert_eq!(result.output["command_started"], true);
    assert_eq!(result.output["command_completed"], true);
    assert_eq!(result.output["command_ok"], false);
    assert_eq!(result.output["failure_kind"], "command_exit_nonzero");
    assert_eq!(result.output["tool_failure"], false);
}

#[tokio::test]
async fn run_shell_rejection_reports_not_started_and_no_files_modified() {
    let result = test_runtime()
        .run_shell(
            "agent:missing:missing".to_string(),
            "printf should-not-run".to_string(),
            Some(30),
            None,
        )
        .await;
    assert!(!result.success);
    let error = result.error.as_deref().unwrap_or("");
    assert!(error.contains("Rejected before starting command"));
    assert!(error.contains("No command was started"));
    assert!(error.contains("No files were modified"));
    assert!(error.contains("Retry guidance"));
    assert_eq!(result.output["command_started"], false);
    assert_eq!(result.output["command_completed"], false);
    assert_eq!(result.output["command_ok"], false);
    assert_eq!(result.output["failure_kind"], "agent_offline");
    assert_eq!(result.output["tool_failure"], true);
}

#[tokio::test]
async fn run_shell_exit_zero_reports_structured_command_success() {
    let runtime = runtime_with_agent_project("shell-ok");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "shell-ok", None, caps).await;
    let project = agent_test_project_id("shell-ok");
    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .run_shell(
                project,
                "printf ok; printf err >&2".to_string(),
                Some(30),
                None,
            )
            .await
    });
    let req = next_patch_agent_request(&runtime, "shell-ok")
        .await
        .expect("run_shell should enqueue a shell command");
    complete_patch_agent_request(&runtime, "shell-ok", &req.request_id, 0, "ok", "err").await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["exit_code"], 0);
    assert_eq!(result.output["stdout"], "ok");
    assert_eq!(result.output["stderr"], "err");
    assert_eq!(result.output["command_started"], true);
    assert_eq!(result.output["command_completed"], true);
    assert_eq!(result.output["command_ok"], true);
    assert!(result.output["failure_kind"].is_null());
    assert_eq!(result.output["tool_failure"], false);
}

#[tokio::test]
async fn run_shell_exit_seven_reports_structured_command_nonzero() {
    let runtime = runtime_with_agent_project("shell-seven");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "shell-seven", None, caps).await;
    let project = agent_test_project_id("shell-seven");
    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .run_shell(
                project,
                "printf out; printf err >&2; exit 7".to_string(),
                Some(30),
                None,
            )
            .await
    });
    let req = next_patch_agent_request(&runtime, "shell-seven")
        .await
        .expect("run_shell should enqueue a shell command");
    complete_patch_agent_request(&runtime, "shell-seven", &req.request_id, 7, "out", "err").await;
    let result = task.await.unwrap();

    assert!(!result.success);
    assert_eq!(result.output["command_started"], true);
    assert_eq!(result.output["command_completed"], true);
    assert_eq!(result.output["command_ok"], false);
    assert_eq!(result.output["exit_code"], 7);
    assert_eq!(result.output["failure_kind"], "command_exit_nonzero");
    assert_eq!(result.output["tool_failure"], false);
    assert_eq!(result.output["stdout_tail"], "out");
    assert_eq!(result.output["stderr_tail"], "err");
}

#[tokio::test]
async fn run_shell_timeout_reports_structured_timeout_failure_kind() {
    let runtime = runtime_with_agent_project("shell-timeout");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "shell-timeout", None, caps).await;
    let project = agent_test_project_id("shell-timeout");
    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .run_shell(project, "sleep 2".to_string(), Some(1), None)
            .await
    });
    let _req = next_patch_agent_request(&runtime, "shell-timeout")
        .await
        .expect("run_shell should enqueue a shell command");
    let result = task.await.unwrap();

    assert!(!result.success);
    assert_eq!(result.output["command_started"], true);
    assert_eq!(result.output["command_completed"], false);
    assert_eq!(result.output["command_ok"], false);
    assert!(result.output["exit_code"].is_null());
    assert_eq!(result.output["failure_kind"], "timeout");
    assert_eq!(result.output["tool_failure"], true);
}

#[tokio::test]
async fn local_job_status_marks_over_time_running_job_lost() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let runtime = runtime_with_project(root, "demo");
    let job_id = "66666666-7777-8888-9999-000000000000";
    let past = chrono::Utc::now().timestamp() - 100_000;
    write_fake_job(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "running",
        "",
        "",
        json!({ "started_at": past, "max_runtime_secs": 60 }),
    );
    let result = runtime.job_status(job_id.to_string()).await;
    assert!(result.success);
    assert_eq!(result.output["status"], "lost");
}

#[tokio::test]
async fn local_job_status_keeps_completed_job_completed() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let runtime = runtime_with_project(root, "demo");
    let job_id = "77777777-8888-9999-0000-111111111111";
    let past = chrono::Utc::now().timestamp() - 100_000;
    // Completed jobs stay completed even if max_runtime would have passed.
    write_fake_job(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "completed",
        "",
        "",
        json!({ "started_at": past, "max_runtime_secs": 60 }),
    );
    let result = runtime.job_status(job_id.to_string()).await;
    assert!(result.success);
    assert_eq!(result.output["status"], "completed");
}

#[tokio::test]
async fn run_job_rejects_server_configured_project_without_local_spawn() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let runtime = runtime_with_project(root, "demo");
    let result = runtime
        .run_job("demo".to_string(), "true".to_string(), None, Some(10), None)
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("unknown_project"));
    assert!(runtime.local_jobs.lock().await.is_empty());
}

#[tokio::test]
async fn timeout_terminates_recorded_process_group() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let (runtime, killer) = runtime_with_fake_killer(root, "demo");
    let job_id = "12121212-3434-5656-7878-909090909090";
    let past = chrono::Utc::now().timestamp() - 100_000;
    let dir = write_fake_job_with_pgid(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "running",
        12345,
        json!({ "started_at": past, "max_runtime_secs": 60, "process_group_id": 12345 }),
    );
    let result = runtime.job_status(job_id.to_string()).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["status"], "lost");
    assert!(result.output["note"]
        .as_str()
        .unwrap()
        .contains("process group 12345"));
    // The recorded pgid was targeted for termination.
    assert_eq!(killer.calls(), vec![(12345, 12345)]);
    // Terminal state persisted to disk.
    assert_eq!(read_trim(dir.join("status")).unwrap(), "lost");
    assert!(read_trim(dir.join("finished_at")).is_some());
}

#[tokio::test]
async fn timeout_without_pid_only_marks_lost_no_kill() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let (runtime, killer) = runtime_with_fake_killer(root, "demo");
    let job_id = "13131313-4545-6767-8989-101010101010";
    let past = chrono::Utc::now().timestamp() - 100_000;
    // No pid file, no process_group_id — simulates very old metadata that
    // predates pid/pgid tracking. We must NOT guess a pid to kill.
    write_fake_job(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "running",
        "",
        "",
        json!({ "started_at": past, "max_runtime_secs": 60 }),
    );
    let result = runtime.job_status(job_id.to_string()).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["status"], "lost");
    // No kill attempted because no pid/pgid was recorded.
    assert!(killer.calls().is_empty());
    assert!(result.output["note"].as_str().unwrap().contains("no pid"));
}

#[tokio::test]
async fn job_log_also_reclaims_timeout_process_group() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let (runtime, killer) = runtime_with_fake_killer(root, "demo");
    let job_id = "14141414-5656-7878-9090-111111111111";
    let past = chrono::Utc::now().timestamp() - 100_000;
    write_fake_job_with_pgid(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "running",
        4242,
        json!({ "started_at": past, "max_runtime_secs": 60, "process_group_id": 4242 }),
    );
    let result = runtime.job_log(job_id.to_string(), None, None).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["status"], "lost");
    assert_eq!(killer.calls(), vec![(4242, 4242)]);
}

#[tokio::test]
async fn timeout_does_not_affect_completed_job() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let (runtime, killer) = runtime_with_fake_killer(root, "demo");
    let job_id = "15151515-6767-8989-1010-121212121212";
    let past = chrono::Utc::now().timestamp() - 100_000;
    write_fake_job_with_pgid(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "completed",
        9999,
        json!({ "started_at": past, "max_runtime_secs": 60, "process_group_id": 9999 }),
    );
    let result = runtime.job_status(job_id.to_string()).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["status"], "completed");
    assert!(killer.calls().is_empty());
}

#[tokio::test]
async fn stop_job_terminates_process_group() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let (runtime, killer) = runtime_with_fake_killer(root, "demo");
    let job_id = "16161616-7878-9090-1111-131313131313";
    let now = chrono::Utc::now().timestamp();
    let dir = write_fake_job_with_pgid(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "running",
        7777,
        json!({ "started_at": now, "max_runtime_secs": 3600, "process_group_id": 7777 }),
    );
    let result = runtime.stop_job(job_id.to_string()).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["status"], "stopped");
    assert_eq!(killer.calls(), vec![(7777, 7777)]);
    assert_eq!(read_trim(dir.join("status")).unwrap(), "stopped");
    assert!(read_trim(dir.join("finished_at")).is_some());
}

#[tokio::test]
async fn stop_job_leaves_completed_job_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let (runtime, killer) = runtime_with_fake_killer(root, "demo");
    let job_id = "17171717-8989-1010-1212-141414141414";
    let past = chrono::Utc::now().timestamp() - 100_000;
    write_fake_job_with_pgid(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "completed",
        8888,
        json!({ "started_at": past, "max_runtime_secs": 60, "process_group_id": 8888 }),
    );
    let result = runtime.stop_job(job_id.to_string()).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["status"], "completed");
    assert!(killer.calls().is_empty());
}

#[tokio::test]
async fn stop_job_rejects_unsafe_job_id() {
    let tmp = tempfile::tempdir().unwrap();
    let (runtime, _killer) = runtime_with_fake_killer(tmp.path(), "demo");
    let result = runtime.stop_job("../escape".to_string()).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("invalid job id"));
}

#[tokio::test]
async fn stop_job_unknown_job_returns_error() {
    let tmp = tempfile::tempdir().unwrap();
    let (runtime, _killer) = runtime_with_fake_killer(tmp.path(), "demo");
    let result = runtime
        .stop_job("55555555-6666-7777-8888-999999999999".to_string())
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("unknown job"));
}

#[tokio::test]
async fn model_facing_stop_job_requires_confirm_without_stopping_or_approving() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let (runtime, killer) = runtime_with_fake_killer(root, "demo");
    let session = runtime
        .sessions
        .start_session(Some("demo".to_string()), Some("stop confirm".to_string()));
    let job_id = "18181818-9090-1111-2222-333333333333";
    let now = chrono::Utc::now().timestamp();
    let dir = write_fake_job_with_pgid(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "running",
        7777,
        json!({
            "started_at": now,
            "max_runtime_secs": 3600,
            "process_group_id": 7777,
            "session_id": session.session_id,
        }),
    );

    let result = runtime
        .dispatch(ToolCall::StopJob {
            project: "demo".to_string(),
            job_id: job_id.to_string(),
            session_id: Some(session.session_id.clone()),
            confirm: false,
        })
        .await;

    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "confirmation_required");
    assert_eq!(result.output["failure_kind"], "confirmation_required");
    assert_eq!(result.output["command_started"], false);
    assert!(result.output.get("permission").is_none());
    assert_eq!(read_trim(dir.join("status")).unwrap(), "running");
    assert!(killer.calls().is_empty());
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    let permissions = crate::tool_runtime::permissions::permission_summary_from_events(
        &summary.events,
        crate::tool_runtime::permissions::DEFAULT_PERMISSION_RECENT_LIMIT,
    );
    assert_eq!(permissions["auto_approved_count"], 0);
}

#[tokio::test]
async fn model_facing_stop_job_stops_local_job_and_records_permission() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let (runtime, killer) = runtime_with_fake_killer(root, "demo");
    let session = runtime
        .sessions
        .start_session(Some("demo".to_string()), Some("stop local".to_string()));
    let job_id = "19191919-0000-1111-2222-333333333333";
    let now = chrono::Utc::now().timestamp();
    let dir = write_fake_job_with_pgid(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "running",
        8888,
        json!({
            "started_at": now,
            "max_runtime_secs": 3600,
            "process_group_id": 8888,
            "session_id": session.session_id,
        }),
    );

    let result = runtime
        .dispatch(ToolCall::StopJob {
            project: "demo".to_string(),
            job_id: job_id.to_string(),
            session_id: Some(session.session_id.clone()),
            confirm: true,
        })
        .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["stopped"], true);
    assert_eq!(result.output["already_finished"], false);
    assert_eq!(result.output["status_before"], "running");
    assert_eq!(result.output["status_after"], "stopped");
    assert_eq!(result.output["ownership_basis"], "project_and_session");
    assert_eq!(result.output["permission"]["status"], "auto_approved");
    assert_eq!(result.output["permission"]["risk"], "job");
    assert_eq!(result.output["permission"]["tool_name"], "stop_job");
    assert_eq!(killer.calls(), vec![(8888, 8888)]);
    assert_eq!(read_trim(dir.join("status")).unwrap(), "stopped");
}

#[tokio::test]
async fn model_facing_stop_job_noops_already_finished_with_permission() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let (runtime, killer) = runtime_with_fake_killer(root, "demo");
    let session = runtime
        .sessions
        .start_session(Some("demo".to_string()), Some("stop done".to_string()));
    let job_id = "20202020-1111-2222-3333-444444444444";
    let past = chrono::Utc::now().timestamp() - 100;
    write_fake_job_with_pgid(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "completed",
        9999,
        json!({
            "started_at": past,
            "max_runtime_secs": 3600,
            "process_group_id": 9999,
            "session_id": session.session_id,
        }),
    );

    let result = runtime
        .dispatch(ToolCall::StopJob {
            project: "demo".to_string(),
            job_id: job_id.to_string(),
            session_id: Some(session.session_id.clone()),
            confirm: true,
        })
        .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["stopped"], false);
    assert_eq!(result.output["already_finished"], true);
    assert_eq!(result.output["status_before"], "completed");
    assert_eq!(result.output["status_after"], "completed");
    assert_eq!(result.output["permission"]["status"], "auto_approved");
    assert_eq!(result.output["permission"]["risk"], "job");
    assert!(killer.calls().is_empty());
}

#[tokio::test]
async fn model_facing_stop_job_allows_unknown_session_with_project_warning() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let (runtime, killer) = runtime_with_fake_killer(root, "demo");
    let job_id = "21212121-2222-3333-4444-555555555555";
    let now = chrono::Utc::now().timestamp();
    write_fake_job_with_pgid(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "running",
        1212,
        json!({ "started_at": now, "max_runtime_secs": 3600, "process_group_id": 1212 }),
    );

    let result = runtime
        .dispatch(ToolCall::StopJob {
            project: "demo".to_string(),
            job_id: job_id.to_string(),
            session_id: None,
            confirm: true,
        })
        .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["stopped"], true);
    assert_eq!(
        result.output["ownership_basis"],
        "unknown_session_project_only"
    );
    assert_eq!(result.output["warning_kind"], "job_session_unknown");
    assert_eq!(result.output["warnings"][0]["kind"], "job_session_unknown");
    assert_eq!(killer.calls(), vec![(1212, 1212)]);
}

#[tokio::test]
async fn model_facing_stop_job_rejects_different_session_before_stop() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let (runtime, killer) = runtime_with_fake_killer(root, "demo");
    let owner_session = runtime
        .sessions
        .start_session(Some("demo".to_string()), Some("owner".to_string()));
    let other_session = runtime
        .sessions
        .start_session(Some("demo".to_string()), Some("other".to_string()));
    let job_id = "22222222-3333-4444-5555-666666666666";
    let now = chrono::Utc::now().timestamp();
    write_fake_job_with_pgid(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "running",
        3434,
        json!({
            "started_at": now,
            "max_runtime_secs": 3600,
            "process_group_id": 3434,
            "session_id": owner_session.session_id,
        }),
    );

    let result = runtime
        .dispatch(ToolCall::StopJob {
            project: "demo".to_string(),
            job_id: job_id.to_string(),
            session_id: Some(other_session.session_id.clone()),
            confirm: true,
        })
        .await;

    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "job_stop_forbidden");
    assert_eq!(result.output["failure_kind"], "job_stop_forbidden");
    assert_eq!(result.output["command_started"], false);
    assert!(result.output.get("permission").is_none());
    assert!(killer.calls().is_empty());
}

#[tokio::test]
async fn model_facing_stop_job_rejects_agent_project_mismatch() {
    let runtime = test_runtime();
    let auth = open_auth_context();
    register_job_agent_for_auth(&runtime, "client-alpha", "proj-alpha", &auth).await;
    register_job_agent_for_auth(&runtime, "client-beta", "proj-beta", &auth).await;
    let job_id = start_agent_runtime_job(&runtime, "client-alpha", "proj-alpha", &auth).await;

    let result = runtime
        .dispatch_with_auth(
            ToolCall::StopJob {
                project: "agent:client-beta:proj-beta".to_string(),
                job_id,
                session_id: None,
                confirm: true,
            },
            Some(&auth),
        )
        .await;

    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "job_project_mismatch");
    assert_eq!(result.output["failure_kind"], "job_project_mismatch");
    assert_eq!(result.output["command_started"], false);
    assert!(result.output.get("permission").is_none());
}

#[tokio::test]
async fn model_facing_stop_job_stops_agent_job_with_same_session() {
    let runtime = test_runtime();
    let auth = open_auth_context();
    register_job_agent_for_auth(&runtime, "client-stop", "proj-stop", &auth).await;
    let project = "agent:client-stop:proj-stop".to_string();
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("agent stop".to_string()));
    let run = runtime
        .dispatch_with_auth(
            ToolCall::RunJob {
                project: project.clone(),
                command: "echo queued".to_string(),
                session_id: Some(session.session_id.clone()),
                timeout_secs: None,
                cwd: None,
            },
            Some(&auth),
        )
        .await;
    assert!(run.success, "{:?}", run.error);
    let job_id = run.output["job_id"].as_str().unwrap().to_string();

    let result = runtime
        .dispatch_with_auth(
            ToolCall::StopJob {
                project,
                job_id: job_id.clone(),
                session_id: Some(session.session_id.clone()),
                confirm: true,
            },
            Some(&auth),
        )
        .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["stopped"], true);
    assert_eq!(result.output["status_before"], "queued");
    assert_eq!(result.output["status_after"], "stopped");
    assert_eq!(result.output["ownership_basis"], "project_and_session");
    assert_eq!(result.output["permission"]["status"], "auto_approved");
    assert_eq!(result.output["permission"]["risk"], "job");
    let status = runtime
        .dispatch_with_auth(
            ToolCall::JobStatus {
                job_id,
                include_command_preview: false,
            },
            Some(&auth),
        )
        .await;
    assert!(status.success, "{:?}", status.error);
    assert_eq!(status.output["status"], "stopped");
}

#[tokio::test]
async fn model_facing_stop_job_session_project_mismatch_beats_auto_approve() {
    let runtime = test_runtime();
    let auth = open_auth_context();
    register_job_agent_for_auth(&runtime, "client-one", "proj-one", &auth).await;
    register_job_agent_for_auth(&runtime, "client-two", "proj-two", &auth).await;
    let session = runtime.sessions.start_session(
        Some("agent:client-one:proj-one".to_string()),
        Some("mismatch".to_string()),
    );

    let outcome = runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: "stop_job".to_string(),
                arguments: json!({
                    "project": "agent:client-two:proj-two",
                    "job_id": "wc_job_not_needed",
                    "session_id": session.session_id,
                    "confirm": true,
                }),
            },
            ToolCallContext {
                transport: ToolTransport::Api,
                session_id: None,
                auth: Some(&auth),
                record_oauth_scope_denials: true,
            },
        )
        .await;
    let result = outcome.result.expect("tool result");

    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "session_project_mismatch");
    assert_eq!(result.output["failure_kind"], "session_project_mismatch");
    assert_eq!(result.output["command_started"], false);
    assert!(result.output.get("permission").is_none());
    let summary = runtime
        .sessions
        .summary(result.output["session_id"].as_str().unwrap(), Some(20))
        .unwrap();
    let event = summary
        .events
        .iter()
        .rev()
        .find(|event| event.kind == "tool_call_finished" && event.tool_name == "stop_job")
        .expect("stop_job finished event");
    assert_eq!(
        event.error_kind.as_deref(),
        Some("session_project_mismatch")
    );
    assert!(event.permission.is_none());
}

#[tokio::test]
async fn job_log_recovery_returns_bounded_output() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let runtime = runtime_with_project(root, "demo");
    let job_id = "88888888-9999-0000-1111-222222222222";
    let stdout = (1..=1000)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    write_fake_job(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "running",
        &stdout,
        "",
        json!({}),
    );
    let result = runtime.job_log(job_id.to_string(), None, None).await;
    assert!(result.success);
    let out = result.output["stdout"].as_str().unwrap();
    let lines: Vec<&str> = out.lines().collect();
    assert!(lines.len() <= MAX_LOCAL_LOG_LINES);
    assert!(out.contains("line 1000"));
    assert!(!out.contains("line 1\n"));
}

#[tokio::test]
async fn job_log_recovery_paginates_with_offset() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let runtime = runtime_with_project(root, "demo");
    let job_id = "99999999-0000-1111-2222-333333333333";
    let stdout = (1..=600)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    write_fake_job(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "running",
        &stdout,
        "",
        json!({}),
    );
    let first = runtime.job_log(job_id.to_string(), Some(1), None).await;
    assert!(first.success);
    let out = first.output["stdout"].as_str().unwrap();
    assert!(out.contains("line 1"));
    assert!(out.contains("line 500"));
    assert!(!out.contains("line 501"));
    assert_eq!(first.output["next_stdout_line"], 501);

    let second = runtime.job_log(job_id.to_string(), Some(501), None).await;
    assert!(second.success);
    let out2 = second.output["stdout"].as_str().unwrap();
    assert!(out2.contains("line 501"));
    assert!(out2.contains("line 600"));
    assert_eq!(second.output["next_stdout_line"], 601);
}

async fn register_job_agent_for_auth(
    runtime: &ToolRuntime,
    client_id: &str,
    project_id: &str,
    auth: &crate::auth::AuthContext,
) {
    let mut caps = ShellClientCapabilities::default();
    caps.async_shell_jobs = true;
    runtime
        .shell_clients
        .register_with_auth(
            ShellClientRegisterRequest {
                client_id: client_id.to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(caps),
                projects: Some(vec![registered_project(
                    project_id,
                    &format!("/tmp/{project_id}"),
                )]),
                agent_protocol_version: Some("polling-v1".to_string()),
                policy: None,
            },
            Some(auth),
        )
        .await
        .unwrap();
}

async fn start_agent_runtime_job(
    runtime: &ToolRuntime,
    client_id: &str,
    project_id: &str,
    auth: &crate::auth::AuthContext,
) -> String {
    let result = runtime
        .dispatch_with_auth(
            ToolCall::RunJob {
                project: format!("agent:{client_id}:{project_id}"),
                command: format!("echo {client_id}"),
                session_id: None,
                timeout_secs: None,
                cwd: None,
            },
            Some(auth),
        )
        .await;
    assert!(result.success, "{:?}", result.error);
    result.output["job_id"].as_str().unwrap().to_string()
}

fn listed_job_ids(result: &ToolResult) -> Vec<String> {
    assert!(result.success, "{:?}", result.error);
    result.output["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|job| job["job_id"].as_str().unwrap().to_string())
        .collect()
}

fn assert_unknown_job(result: ToolResult) {
    assert!(!result.success, "unexpected success: {:?}", result.output);
    assert!(
        result.error.unwrap_or_default().contains("unknown job"),
        "unauthorized job lookup should be hidden as unknown"
    );
}

#[tokio::test]
async fn runtime_job_tools_filter_agent_jobs_by_auth_group() {
    let runtime = test_runtime();
    let shared_a = shared_key_auth_context("hash-a");
    let shared_b = shared_key_auth_context("hash-b");
    let bridge_a = oauth_bridge_auth_context("hash-a", &[crate::auth::SCOPE_JOB_RUN]);
    let bridge_b = oauth_bridge_auth_context("hash-b", &[crate::auth::SCOPE_JOB_RUN]);
    let open = open_auth_context();
    let bootstrap = bootstrap_auth_context();

    register_job_agent_for_auth(&runtime, "client-a", "proj-a", &shared_a).await;
    register_job_agent_for_auth(&runtime, "client-b", "proj-b", &shared_b).await;
    register_job_agent_for_auth(&runtime, "client-open", "proj-open", &open).await;

    let job_a = start_agent_runtime_job(&runtime, "client-a", "proj-a", &shared_a).await;
    let job_b = start_agent_runtime_job(&runtime, "client-b", "proj-b", &shared_b).await;
    let job_open = start_agent_runtime_job(&runtime, "client-open", "proj-open", &open).await;

    let req = next_agent_request_for_instance(&runtime, "client-b", "inst")
        .await
        .expect("client-b job request should be queued");
    complete_patch_agent_request(&runtime, "client-b", &req.request_id, 0, "b-out", "b-err").await;

    let list_a = runtime
        .dispatch_with_auth(
            ToolCall::ListJobs {
                limit: None,
                status: None,
            },
            Some(&shared_a),
        )
        .await;
    assert_eq!(listed_job_ids(&list_a), vec![job_a.clone()]);

    let list_bridge = runtime
        .dispatch_with_auth(
            ToolCall::ListJobs {
                limit: None,
                status: None,
            },
            Some(&bridge_a),
        )
        .await;
    assert_eq!(listed_job_ids(&list_bridge), vec![job_a.clone()]);

    let list_bridge_b = runtime
        .dispatch_with_auth(
            ToolCall::ListJobs {
                limit: None,
                status: None,
            },
            Some(&bridge_b),
        )
        .await;
    assert_eq!(listed_job_ids(&list_bridge_b), vec![job_b.clone()]);

    let list_open = runtime
        .dispatch_with_auth(
            ToolCall::ListJobs {
                limit: None,
                status: None,
            },
            Some(&open),
        )
        .await;
    assert_eq!(listed_job_ids(&list_open), vec![job_open.clone()]);

    let list_bootstrap = runtime
        .dispatch_with_auth(
            ToolCall::ListJobs {
                limit: None,
                status: None,
            },
            Some(&bootstrap),
        )
        .await;
    let mut bootstrap_ids = listed_job_ids(&list_bootstrap);
    bootstrap_ids.sort();
    let mut expected = vec![job_a.clone(), job_b.clone(), job_open.clone()];
    expected.sort();
    assert_eq!(bootstrap_ids, expected);

    assert_unknown_job(
        runtime
            .dispatch_with_auth(
                ToolCall::JobStatus {
                    job_id: job_b.clone(),
                    include_command_preview: false,
                },
                Some(&shared_a),
            )
            .await,
    );
    assert_unknown_job(
        runtime
            .dispatch_with_auth(
                ToolCall::JobStatus {
                    job_id: job_a.clone(),
                    include_command_preview: false,
                },
                Some(&bridge_b),
            )
            .await,
    );
    assert_unknown_job(
        runtime
            .dispatch_with_auth(
                ToolCall::JobStatus {
                    job_id: job_b.clone(),
                    include_command_preview: false,
                },
                Some(&bridge_a),
            )
            .await,
    );
    assert_unknown_job(
        runtime
            .dispatch_with_auth(
                ToolCall::JobLog {
                    job_id: job_b.clone(),
                    offset: None,
                    tail_lines: None,
                },
                Some(&shared_a),
            )
            .await,
    );
    assert_unknown_job(
        runtime
            .dispatch_with_auth(
                ToolCall::JobTail {
                    job_id: job_b.clone(),
                    tail_lines: None,
                },
                Some(&shared_a),
            )
            .await,
    );

    let status_b = runtime
        .dispatch_with_auth(
            ToolCall::JobStatus {
                job_id: job_b.clone(),
                include_command_preview: false,
            },
            Some(&shared_b),
        )
        .await;
    assert!(status_b.success, "{:?}", status_b.error);
    assert_eq!(status_b.output["job_id"], job_b);
    assert!(status_b.output.get("command_preview").is_none());

    let status_b_debug = runtime
        .dispatch_with_auth(
            ToolCall::JobStatus {
                job_id: job_b.clone(),
                include_command_preview: true,
            },
            Some(&shared_b),
        )
        .await;
    assert!(status_b_debug.success, "{:?}", status_b_debug.error);
    assert!(status_b_debug.output["command_preview"]
        .as_str()
        .unwrap()
        .contains("echo client-b"));

    let log_b = runtime
        .dispatch_with_auth(
            ToolCall::JobLog {
                job_id: job_b.clone(),
                offset: None,
                tail_lines: None,
            },
            Some(&shared_b),
        )
        .await;
    assert!(log_b.success, "{:?}", log_b.error);
    assert_eq!(log_b.output["stdout"], "b-out\n");

    let tail_b = runtime
        .dispatch_with_auth(
            ToolCall::JobTail {
                job_id: job_b,
                tail_lines: Some(10),
            },
            Some(&shared_b),
        )
        .await;
    assert!(tail_b.success, "{:?}", tail_b.error);
    assert_eq!(tail_b.output["stdout"], "b-out\n");
}

#[tokio::test]
async fn lightweight_auth_cannot_enumerate_unrelated_local_jobs() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let runtime = runtime_with_project(root, "demo");
    let dir = write_fake_job(
        root,
        "job-local",
        "demo",
        &root.to_string_lossy(),
        "running",
        "local-out",
        "local-err",
        json!({}),
    );
    runtime.local_jobs.lock().await.insert(
        "job-local".to_string(),
        LocalJobRecord {
            project: "demo".to_string(),
            dir,
        },
    );
    let shared = shared_key_auth_context("hash-local");
    let bridge = oauth_bridge_auth_context("hash-local", &[crate::auth::SCOPE_JOB_RUN]);
    let open = open_auth_context();
    let managed_oauth = managed_oauth_auth_context("alice", Some("hash-local"));
    let bootstrap = bootstrap_auth_context();

    for auth in [&shared, &bridge, &open] {
        let result = runtime
            .dispatch_with_auth(
                ToolCall::ListJobs {
                    limit: None,
                    status: None,
                },
                Some(auth),
            )
            .await;
        assert!(listed_job_ids(&result).is_empty());
        assert_unknown_job(
            runtime
                .dispatch_with_auth(
                    ToolCall::JobStatus {
                        job_id: "job-local".to_string(),
                        include_command_preview: false,
                    },
                    Some(auth),
                )
                .await,
        );
    }

    let result = runtime
        .dispatch_with_auth(
            ToolCall::ListJobs {
                limit: None,
                status: None,
            },
            Some(&managed_oauth),
        )
        .await;
    assert_eq!(listed_job_ids(&result), vec!["job-local".to_string()]);
    let status = runtime
        .dispatch_with_auth(
            ToolCall::JobStatus {
                job_id: "job-local".to_string(),
                include_command_preview: false,
            },
            Some(&managed_oauth),
        )
        .await;
    assert!(status.success, "{:?}", status.error);
    assert_eq!(status.output["job_id"], "job-local");

    let result = runtime
        .dispatch_with_auth(
            ToolCall::ListJobs {
                limit: None,
                status: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert_eq!(listed_job_ids(&result), vec!["job-local".to_string()]);
}

#[tokio::test]
async fn runtime_status_filters_job_counts_by_auth_group() {
    let runtime = test_runtime();
    let shared_a = shared_key_auth_context("hash-a");
    let shared_b = shared_key_auth_context("hash-b");
    let open = open_auth_context();
    let bootstrap = bootstrap_auth_context();

    register_job_agent_for_auth(&runtime, "status-a", "proj-a", &shared_a).await;
    register_job_agent_for_auth(&runtime, "status-b", "proj-b", &shared_b).await;
    register_job_agent_for_auth(&runtime, "status-open", "proj-open", &open).await;

    let _job_a = start_agent_runtime_job(&runtime, "status-a", "proj-a", &shared_a).await;
    let _job_b = start_agent_runtime_job(&runtime, "status-b", "proj-b", &shared_b).await;
    let _job_open = start_agent_runtime_job(&runtime, "status-open", "proj-open", &open).await;

    let status_a = runtime
        .dispatch_with_auth(ToolCall::RuntimeStatus, Some(&shared_a))
        .await;
    assert!(status_a.success, "{:?}", status_a.error);
    assert_eq!(status_a.output["jobs"]["agent_known_count"], 1);
    assert_eq!(status_a.output["jobs"]["active_count"], 1);

    let status_open = runtime
        .dispatch_with_auth(ToolCall::RuntimeStatus, Some(&open))
        .await;
    assert!(status_open.success, "{:?}", status_open.error);
    assert_eq!(status_open.output["jobs"]["agent_known_count"], 1);
    assert_eq!(status_open.output["jobs"]["active_count"], 1);

    let status_bootstrap = runtime
        .dispatch_with_auth(ToolCall::RuntimeStatus, Some(&bootstrap))
        .await;
    assert!(status_bootstrap.success, "{:?}", status_bootstrap.error);
    assert_eq!(status_bootstrap.output["jobs"]["agent_known_count"], 3);
    assert_eq!(status_bootstrap.output["jobs"]["active_count"], 3);
}

#[tokio::test]
async fn list_jobs_returns_bounded_summaries_without_stdout_stderr() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let runtime = runtime_with_project(root, "demo");
    // Seed a local job whose on-disk logs contain sensitive-looking text.
    let dir = write_fake_job(
        root,
        "job-secret",
        "demo",
        &root.to_string_lossy(),
        "completed",
        "WEBCODEX_TOKEN=supersecret\nline2",
        "Authorization: Bearer xyz",
        json!({}),
    );
    runtime.local_jobs.lock().await.insert(
        "job-secret".to_string(),
        LocalJobRecord {
            project: "demo".to_string(),
            dir,
        },
    );
    let result = runtime
        .dispatch(ToolCall::ListJobs {
            limit: None,
            status: None,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    let jobs = result.output["jobs"].as_array().unwrap();
    assert_eq!(jobs.len(), 1);
    let job = &jobs[0];
    assert_eq!(job["job_id"], "job-secret");
    assert_eq!(job["status"], "completed");
    assert_eq!(job["executor"], "local");
    // Summaries must never carry stdout/stderr bodies.
    assert!(
        job.get("stdout").is_none(),
        "list_jobs summary must not include stdout"
    );
    assert!(
        job.get("stderr").is_none(),
        "list_jobs summary must not include stderr"
    );
    // And the serialized summary must not leak the secret log text.
    let serialized = serde_json::to_string(job).unwrap();
    assert!(
        !serialized.contains("supersecret"),
        "list_jobs summary leaked stdout secret: {}",
        serialized
    );
    assert!(
        !serialized.contains("Bearer xyz"),
        "list_jobs summary leaked stderr secret: {}",
        serialized
    );
}

#[tokio::test]
async fn list_jobs_respects_limit_bound() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let runtime = runtime_with_project(root, "demo");
    for i in 0..5 {
        let dir = write_fake_job(
            root,
            &format!("job-{}", i),
            "demo",
            &root.to_string_lossy(),
            "completed",
            "",
            "",
            json!({}),
        );
        runtime.local_jobs.lock().await.insert(
            format!("job-{}", i),
            LocalJobRecord {
                project: "demo".to_string(),
                dir,
            },
        );
    }
    let result = runtime
        .dispatch(ToolCall::ListJobs {
            limit: Some(2),
            status: None,
        })
        .await;
    assert!(result.success);
    let jobs = result.output["jobs"].as_array().unwrap();
    assert_eq!(jobs.len(), 2);
    assert_eq!(result.output["truncated"], true);
}

#[tokio::test]
async fn list_jobs_requires_no_agent_capability() {
    // list_jobs has no project and no agent capability requirement, so it
    // succeeds even with no registered agent.
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ListJobs {
            limit: None,
            status: None,
        })
        .await;
    assert!(result.success);
    assert!(result.output["jobs"].is_array());
}

#[tokio::test]
async fn job_tail_reaches_job_logic_without_agent_auth() {
    // job_tail bypasses agent authorization (no project). An unknown job
    // returns a structured "unknown job" error, proving it reached the job
    // layer rather than an authorization gate.
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::JobTail {
            job_id: "no-such-job".to_string(),
            tail_lines: None,
        })
        .await;
    assert!(!result.success);
    assert!(
        result.error.unwrap().contains("unknown job"),
        "job_tail should report unknown job"
    );
}

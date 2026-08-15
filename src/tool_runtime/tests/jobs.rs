//! Jobs tests for tool_runtime.

use super::super::helpers::*;
use super::super::kernel::{ToolCallContext, ToolCallRequest, ToolTransport};
use super::super::ToolRuntime;
use super::super::*;
use super::support::*;
use crate::shell_protocol::{
    ShellAgentJobUpdateRequest, ShellAgentResultPayload, ShellAgentResultRequest,
    ShellClientCapabilities, ShellClientRegisterRequest, ShellCommandExecutionState,
};
use serde_json::json;
use std::fs;
use std::io::Write;

async fn seed_local_job(
    runtime: &ToolRuntime,
    job_id: &str,
    project: &str,
    dir: std::path::PathBuf,
) {
    let record = LocalJobRecord::initialize(project.to_string(), dir)
        .map(|(record, _)| record)
        .unwrap();
    runtime
        .local_jobs
        .lock()
        .await
        .insert(job_id.to_string(), record);
}

#[tokio::test]
async fn run_shell_session_events_record_exit_without_stdio_bodies() {
    let runtime = runtime_with_agent_project("telemetry-shell");
    let caps = ShellClientCapabilities {
        shell: true,
        ..Default::default()
    };
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
                        command: "printf success-output".to_string(),
                        session_id: Some(session_id),
                        timeout_secs: Some(30),
                        cwd: None,
                        purpose: None,
                        shell: None,
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
    assert_eq!(ok.output["permission"]["policy"], "trusted_agent");
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
                        command: "printf failure-output; exit 7".to_string(),
                        session_id: Some(session_id),
                        timeout_secs: Some(30),
                        cwd: None,
                        purpose: None,
                        shell: None,
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
    assert_eq!(permission_summary["manual_approved_count"], 0);
    assert_eq!(permission_summary["approved_count"], 0);
    assert_eq!(permission_summary["total_approved_count"], 2);
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
    assert_eq!(normalize_local_status("started"), "started");
    assert_eq!(normalize_local_status("stop_requested"), "stop_requested");
    assert_eq!(normalize_local_status("lost"), "lost");
    assert_eq!(normalize_local_status("timeout"), "timeout");
    assert_eq!(normalize_local_status("timed_out"), "timed_out");
    assert_eq!(normalize_local_status("cancelled"), "cancelled");
    assert_eq!(normalize_local_status("  failed  "), "failed");
    assert_eq!(normalize_local_status(""), "running");
    assert_eq!(normalize_local_status("weird-state"), "lost");
}

/// Write a log file with `lines` lines of `line N` (1-based) and return the
/// tempdir guard plus the file path.
fn log_fixture(lines: usize) -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("stdout.log");
    let content = (1..=lines)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&path, &content).unwrap();
    (tmp, path)
}

#[test]
fn read_lines_from_bounds_offsets_and_tails() {
    struct Case {
        name: &'static str,
        file_lines: usize,
        offset: Option<usize>,
        tail: Option<usize>,
        expect_len: usize,
        expect_first: Option<&'static str>,
        expect_last: Option<&'static str>,
        expect_next: Option<usize>,
    }
    let cases = [
        // Default is a compact tail, below the explicit per-page maximum.
        Case {
            name: "default_is_bounded_tail",
            file_lines: 1000,
            offset: None,
            tail: None,
            expect_len: 200,
            expect_first: Some("line 801"),
            expect_last: Some("line 1000"),
            expect_next: Some(1001),
        },
        Case {
            name: "offset_first_page",
            file_lines: 600,
            offset: Some(1),
            tail: None,
            expect_len: MAX_LOCAL_LOG_LINES,
            expect_first: Some("line 1"),
            expect_last: Some("line 500"),
            expect_next: Some(501),
        },
        Case {
            name: "offset_second_page",
            file_lines: 600,
            offset: Some(501),
            tail: None,
            expect_len: 100,
            expect_first: Some("line 501"),
            expect_last: Some("line 600"),
            expect_next: Some(601),
        },
        Case {
            name: "tail_lines",
            file_lines: 1000,
            offset: None,
            tail: Some(10),
            expect_len: 10,
            expect_first: Some("line 991"),
            expect_last: Some("line 1000"),
            expect_next: None,
        },
        // Requesting more than MAX returns at most MAX.
        Case {
            name: "tail_capped_to_max",
            file_lines: 1000,
            offset: None,
            tail: Some(5000),
            expect_len: MAX_LOCAL_LOG_LINES,
            expect_first: None,
            expect_last: None,
            expect_next: None,
        },
    ];
    for case in cases {
        let (_tmp, path) = log_fixture(case.file_lines);
        let (text, next, _, _) = read_lines_from(path, case.offset, case.tail);
        let lines: Vec<&str> = text.lines().collect();
        assert!(
            lines.len() <= MAX_LOCAL_LOG_LINES,
            "[{}] output must stay bounded, got {} lines",
            case.name,
            lines.len()
        );
        assert_eq!(lines.len(), case.expect_len, "[{}] line count", case.name);
        if let Some(first) = case.expect_first {
            assert_eq!(lines[0], first, "[{}] first line", case.name);
        }
        if let Some(last) = case.expect_last {
            assert_eq!(lines.last().unwrap(), &last, "[{}] last line", case.name);
        }
        if let Some(expected_next) = case.expect_next {
            assert_eq!(next, expected_next, "[{}] next cursor", case.name);
        }
    }
}

#[tokio::test]
async fn disk_local_job_status_is_not_recovered_without_server_project_map() {
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
    // local_jobs is empty (simulating restart); the runtime no longer scans
    // server-configured project roots, so disk-only jobs are not recovered.
    assert!(runtime.local_jobs.lock().await.is_empty());
    let result = runtime.job_status(job_id.to_string()).await;
    assert!(!result.success, "{:?}", result.output);
    assert!(result.error.unwrap().contains("unknown job"));
    assert!(!runtime.local_jobs.lock().await.contains_key(job_id));
}

#[tokio::test]
async fn job_status_reports_lifecycle_fields_and_bounded_preview_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let runtime = runtime_with_project(root, "demo");
    let job_id = "12121212-3434-5656-7878-909090909090";
    let now = chrono::Utc::now().timestamp();
    let dir = write_fake_job(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "running",
        "stdout secret body\n",
        "stderr secret body\n",
        json!({ "started_at": now, "max_runtime_secs": 3600 }),
    );
    seed_local_job(&runtime, job_id, "demo", dir).await;

    let result = runtime.job_status(job_id.to_string()).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["status"], "running");
    assert_eq!(result.output["active"], true);
    assert_eq!(result.output["blocking_active"], true);
    assert_eq!(result.output["terminal"], false);
    assert_eq!(result.output["terminal_pending"], false);
    assert_eq!(result.output["command_preview_included"], false);
    assert!(result.output.get("command_preview").is_none());
    assert!(result.output.get("stdout").is_none());
    assert!(result.output.get("stderr").is_none());

    let debug_result = runtime
        .dispatch(ToolCall::JobStatus {
            job_id: job_id.to_string(),
            include_command_preview: true,
        })
        .await;
    assert!(debug_result.success, "{:?}", debug_result.error);
    assert_eq!(debug_result.output["command_preview_included"], true);
    assert_eq!(debug_result.output["command_preview"], "echo test");
    assert_eq!(debug_result.output["command_preview_truncated"], false);
    assert!(debug_result.output["command_preview_max_chars"].is_number());
    assert_eq!(debug_result.output["command_preview_bounded"], true);
    assert!(debug_result.output.get("stdout").is_none());
    assert!(debug_result.output.get("stderr").is_none());
}

#[tokio::test]
async fn local_job_log_reads_in_memory_local_job() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let runtime = runtime_with_project(root, "demo");
    let job_id = "22222222-3333-4444-5555-666666666666";
    let dir = write_fake_job(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "running",
        "stdout line\n",
        "stderr line\n",
        json!({}),
    );
    seed_local_job(&runtime, job_id, "demo", dir).await;
    let result = runtime.job_log(job_id.to_string(), None, None).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["stdout_tail"], "stdout line");
    assert_eq!(result.output["stderr_tail"], "stderr line");
    assert!(result.output["cursor"]["stdout"].is_number());
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

/// Drive `run_shell` end to end against a freshly registered shell-capable
/// agent: spawn the call, wait for the enqueued agent request, and complete it
/// with the given `(exit_code, stdout, stderr)`. Passing `completion: None`
/// leaves the request unanswered (e.g. to exercise the run_shell timeout).
async fn run_shell_via_agent(
    client_id: &str,
    command: &str,
    timeout_secs: Option<u64>,
    completion: Option<(i32, &str, &str)>,
) -> ToolResult {
    let runtime = runtime_with_agent_project(client_id);
    let caps = ShellClientCapabilities {
        shell: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);
    let runtime_for_task = runtime.clone();
    let command = command.to_string();
    let task = tokio::spawn(async move {
        runtime_for_task
            .run_shell(project, command, timeout_secs, None)
            .await
    });
    let req = next_patch_agent_request(&runtime, client_id)
        .await
        .expect("run_shell should enqueue a shell command");
    if let Some((exit_code, stdout, stderr)) = completion {
        complete_patch_agent_request(
            &runtime,
            client_id,
            &req.request_id,
            exit_code,
            stdout,
            stderr,
        )
        .await;
    }
    task.await.unwrap()
}

async fn run_shell_via_agent_lifecycle_error(
    client_id: &str,
    error: &str,
    execution_state: ShellCommandExecutionState,
) -> ToolResult {
    let runtime = runtime_with_agent_project(client_id);
    let caps = ShellClientCapabilities {
        shell: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);
    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .run_shell(project, "printf lifecycle".to_string(), Some(30), None)
            .await
    });
    let request = next_patch_agent_request(&runtime, client_id)
        .await
        .expect("run_shell should be dispatched");
    runtime
        .shell_clients
        .complete(ShellAgentResultPayload {
            result: ShellAgentResultRequest {
                client_id: client_id.to_string(),
                agent_instance_id: "inst".to_string(),
                request_id: request.request_id,
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(1),
                error: Some(error.to_string()),
            },
            command_execution_state: Some(execution_state),
        })
        .await
        .unwrap();
    task.await.unwrap()
}

#[tokio::test]
async fn run_shell_failure_reports_command_started_and_output_tail() {
    let result = run_shell_via_agent(
        "shell-failer",
        "printf run-shell-out; printf run-shell-err >&2; exit 7",
        Some(30),
        Some((7, "run-shell-out", "run-shell-err")),
    )
    .await;
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
    assert_eq!(result.output["execution_state"], "completed");
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
    assert_eq!(result.output["execution_state"], "not_started");
    assert_eq!(result.output["failure_kind"], "agent_offline");
    assert_eq!(result.output["tool_failure"], true);
}

#[tokio::test]
async fn run_shell_runner_pre_spawn_error_reports_not_started() {
    let result = run_shell_via_agent_lifecycle_error(
        "shell-pre-spawn-error",
        "failed to spawn command: executable not found",
        ShellCommandExecutionState::NotStarted,
    )
    .await;

    assert!(!result.success);
    let error = result.error.as_deref().unwrap_or_default();
    assert_eq!(result.output["command_started"], false);
    assert_eq!(result.output["command_completed"], false);
    assert_eq!(result.output["execution_state"], "not_started");
    assert!(error.contains("No command was started"));
    assert!(error.contains("No files were modified"));
}

#[tokio::test]
async fn run_shell_runner_post_spawn_output_error_reports_unknown() {
    let result = run_shell_via_agent_lifecycle_error(
        "shell-post-spawn-error",
        "stdout reader did not finish before cleanup deadline",
        ShellCommandExecutionState::OutcomeUnknown,
    )
    .await;

    assert!(!result.success);
    let error = result.error.as_deref().unwrap_or_default();
    assert_eq!(result.output["command_started"], true);
    assert_eq!(result.output["command_completed"], false);
    assert_eq!(result.output["execution_state"], "outcome_unknown");
    assert_eq!(result.output["failure_kind"], "outcome_unknown");
    assert!(error.contains("Do not automatically retry"));
    assert!(!error.contains("No command was started"));
    assert!(!error.contains("No files were modified"));
}

#[tokio::test]
async fn run_shell_exit_codes_report_structured_command_results() {
    struct Case {
        name: &'static str,
        client_id: &'static str,
        command: &'static str,
        exit_code: i32,
        stdout: &'static str,
        stderr: &'static str,
        expect_success: bool,
        expect_command_ok: bool,
        // None => failure_kind must be JSON null.
        expect_failure_kind: Option<&'static str>,
        // Success reports full bodies (stdout/stderr); nonzero exits report
        // bounded tails (stdout_tail/stderr_tail).
        stdout_field: &'static str,
        stderr_field: &'static str,
    }
    let cases = [
        Case {
            name: "exit_zero",
            client_id: "shell-ok",
            command: "printf ok; printf err >&2",
            exit_code: 0,
            stdout: "ok",
            stderr: "err",
            expect_success: true,
            expect_command_ok: true,
            expect_failure_kind: None,
            stdout_field: "stdout_tail",
            stderr_field: "stderr_tail",
        },
        Case {
            name: "exit_seven",
            client_id: "shell-seven",
            command: "printf out; printf err >&2; exit 7",
            exit_code: 7,
            stdout: "out",
            stderr: "err",
            expect_success: false,
            expect_command_ok: false,
            expect_failure_kind: Some("command_exit_nonzero"),
            stdout_field: "stdout_tail",
            stderr_field: "stderr_tail",
        },
    ];
    for case in cases {
        let result = run_shell_via_agent(
            case.client_id,
            case.command,
            Some(30),
            Some((case.exit_code, case.stdout, case.stderr)),
        )
        .await;

        assert_eq!(
            result.success, case.expect_success,
            "[{}] success flag, error: {:?}",
            case.name, result.error
        );
        assert_eq!(
            result.output["exit_code"], case.exit_code,
            "[{}] exit_code",
            case.name
        );
        assert_eq!(
            result.output[case.stdout_field], case.stdout,
            "[{}] {}",
            case.name, case.stdout_field
        );
        assert_eq!(
            result.output[case.stderr_field], case.stderr,
            "[{}] {}",
            case.name, case.stderr_field
        );
        assert_eq!(
            result.output["command_started"], true,
            "[{}] command_started",
            case.name
        );
        assert_eq!(
            result.output["command_completed"], true,
            "[{}] command_completed",
            case.name
        );
        assert_eq!(
            result.output["execution_state"], "completed",
            "[{}] execution_state",
            case.name
        );
        assert_eq!(
            result.output["command_ok"], case.expect_command_ok,
            "[{}] command_ok",
            case.name
        );
        match case.expect_failure_kind {
            Some(kind) => assert_eq!(
                result.output["failure_kind"], kind,
                "[{}] failure_kind",
                case.name
            ),
            None => assert!(
                result.output["failure_kind"].is_null(),
                "[{}] failure_kind should be null, got {:?}",
                case.name,
                result.output["failure_kind"]
            ),
        }
        assert_eq!(
            result.output["tool_failure"], false,
            "[{}] tool_failure",
            case.name
        );
    }
}

#[tokio::test]
async fn run_shell_result_wait_timeout_reports_unknown_outcome() {
    // The enqueued agent request is intentionally never completed, so the
    // result wait expires after dispatch without proving the command's final
    // outcome.
    let result = run_shell_via_agent("shell-timeout", "sleep 2", Some(1), None).await;

    assert!(!result.success);
    let error = result.error.as_deref().unwrap_or_default();
    assert!(error.contains("Command execution outcome is unknown"));
    assert!(error.contains("Do not automatically retry"));
    assert!(error.contains("inspect the actual Job, process, service, or target state"));
    assert!(!error.contains("No command was started"));
    assert!(!error.contains("No files were modified"));
    assert_eq!(result.output["command_started"], true);
    assert_eq!(result.output["command_completed"], false);
    assert_eq!(result.output["command_ok"], false);
    assert!(result.output["exit_code"].is_null());
    assert_eq!(result.output["execution_state"], "outcome_unknown");
    assert_eq!(result.output["failure_kind"], "outcome_unknown");
    assert_eq!(result.output["tool_failure"], true);
}

#[tokio::test]
async fn run_shell_runner_timeout_preserves_known_timeout_state() {
    let client_id = "shell-runner-timeout";
    let runtime = runtime_with_agent_project(client_id);
    let caps = ShellClientCapabilities {
        shell: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);
    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .run_shell(project, "sleep 2".to_string(), Some(1), None)
            .await
    });
    let request = next_patch_agent_request(&runtime, client_id)
        .await
        .expect("run_shell should be dispatched");
    runtime
        .shell_clients
        .complete(ShellAgentResultPayload {
            result: ShellAgentResultRequest {
                client_id: client_id.to_string(),
                agent_instance_id: "inst".to_string(),
                request_id: request.request_id,
                exit_code: Some(-1),
                stdout: Some("partial output".to_string()),
                stderr: Some("runner stopped the process at its deadline".to_string()),
                duration_ms: Some(1_000),
                error: Some("runner timeout".to_string()),
            },
            command_execution_state: Some(ShellCommandExecutionState::TimedOut),
        })
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["command_started"], true);
    assert_eq!(result.output["command_completed"], false);
    assert_eq!(result.output["execution_state"], "timed_out");
    assert_eq!(result.output["failure_kind"], "timeout");
    assert_eq!(result.output["tool_failure"], false);
    let error = result.error.as_deref().unwrap_or_default();
    assert!(error.contains("Command timed out after 1s"));
    assert!(error.contains("do not blindly retry"));
    assert!(error.contains("First inspect the actual process, service, and target state"));
}

#[tokio::test]
async fn run_shell_transport_disconnect_after_dispatch_reports_unknown_outcome() {
    let client_id = "shell-disconnect";
    let runtime = runtime_with_agent_project(client_id);
    let caps = ShellClientCapabilities {
        shell: true,
        ..Default::default()
    };
    register_agent(&runtime, client_id, None, caps).await;
    let project = agent_test_project_id(client_id);
    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .run_shell(project, "printf possibly-ran".to_string(), Some(30), None)
            .await
    });
    next_patch_agent_request(&runtime, client_id)
        .await
        .expect("run_shell should be dispatched before disconnect");

    runtime
        .shell_clients
        .reconcile_disconnect(client_id, "inst")
        .await;

    let result = task.await.unwrap();
    let error = result.error.as_deref().unwrap_or_default();
    assert!(!result.success);
    assert_eq!(result.output["command_started"], true);
    assert_eq!(result.output["command_completed"], false);
    assert_eq!(result.output["execution_state"], "outcome_unknown");
    assert_eq!(result.output["failure_kind"], "outcome_unknown");
    assert!(error.contains("Command execution outcome is unknown"));
    assert!(error.contains("Do not automatically retry"));
    assert!(!error.contains("No command was started"));
    assert!(!error.contains("No files were modified"));
}

#[tokio::test]
async fn local_job_status_marks_over_time_running_job_lost() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let runtime = runtime_with_project(root, "demo");
    let job_id = "66666666-7777-8888-9999-000000000000";
    let past = chrono::Utc::now().timestamp() - 100_000;
    let dir = write_fake_job(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "running",
        "",
        "",
        json!({ "started_at": past, "max_runtime_secs": 60 }),
    );
    seed_local_job(&runtime, job_id, "demo", dir).await;
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
    let dir = write_fake_job(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "completed",
        "",
        "",
        json!({ "started_at": past, "max_runtime_secs": 60 }),
    );
    seed_local_job(&runtime, job_id, "demo", dir).await;
    let result = runtime.job_status(job_id.to_string()).await;
    assert!(result.success);
    assert_eq!(result.output["status"], "completed");
}

#[tokio::test]
async fn console_list_does_not_promote_stale_run_job_handoff_after_authoritative_completion() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let runtime = runtime_with_project(root, "demo");
    let session = runtime.sessions.start_session(
        Some("demo".to_string()),
        Some("completed async job".to_string()),
    );
    let job_id = "67676767-7878-8989-9090-101010101010";

    let handoff = runtime.sessions.record_tool_call_started(
        Some(&session.session_id),
        crate::tool_runtime::sessions::SessionTransport::Api,
        "run_job",
        &json!({"project": "demo"}),
    );
    runtime.sessions.record_tool_call_finished(
        handoff,
        true,
        &json!({
            "execution_state": "started",
            "job_id": job_id,
            "status": "running",
            "stdout_tail": "",
            "stderr_tail": "",
            "stdout_lines": 0,
            "stderr_lines": 0
        }),
        None,
        None,
    );

    let past = chrono::Utc::now().timestamp() - 5;
    let dir = write_fake_job(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "completed",
        "",
        "",
        json!({
            "started_at": past,
            "max_runtime_secs": 60,
            "session_id": session.session_id,
        }),
    );
    seed_local_job(&runtime, job_id, "demo", dir).await;
    let terminal = runtime.job_status(job_id.to_string()).await;
    assert!(terminal.success, "{:?}", terminal.error);
    assert_eq!(terminal.output["status"], "completed");

    let detail = runtime
        .sessions
        .console_detail_for_project("demo", &session.session_id, Some(20))
        .unwrap();
    let stale_handoff = detail
        .activity
        .iter()
        .find(|activity| activity.tool.as_deref() == Some("run_job"))
        .unwrap();
    assert_eq!(stale_handoff.execution_state.as_deref(), Some("started"));

    let list = runtime.sessions.console_list_for_project("demo", Some(10));
    let row = list
        .sessions
        .iter()
        .find(|row| row.session_id == session.session_id)
        .unwrap();
    assert!(row.current_activity.is_none());
    assert_eq!(
        row.last_activity.as_ref().unwrap().tool.as_deref(),
        Some("run_job")
    );

    let observed = runtime.sessions.record_tool_call_started(
        Some(&session.session_id),
        crate::tool_runtime::sessions::SessionTransport::Api,
        "job_status",
        &json!({"project": "demo", "job_id": job_id}),
    );
    runtime
        .sessions
        .record_tool_call_finished(observed, true, &terminal.output, None, None);
    let list = runtime.sessions.console_list_for_project("demo", Some(10));
    let row = list
        .sessions
        .iter()
        .find(|row| row.session_id == session.session_id)
        .unwrap();
    assert!(row.current_activity.is_none());
    assert_eq!(
        row.last_activity.as_ref().unwrap().tool.as_deref(),
        Some("job_status")
    );

    runtime.sessions.close_session(&session.session_id).unwrap();
    let list = runtime.sessions.console_list_for_project("demo", Some(10));
    let row = list
        .sessions
        .iter()
        .find(|row| row.session_id == session.session_id)
        .unwrap();
    assert_eq!(row.lifecycle, "closed");
    assert!(row.current_activity.is_none());
}

#[tokio::test]
async fn run_job_rejects_server_configured_project_without_local_spawn() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let runtime = runtime_with_project(root, "demo");
    let result = runtime
        .run_job_for_auth(
            "demo".to_string(),
            "true".to_string(),
            None,
            Some(10),
            None,
            Vec::new(),
            None,
            None,
        )
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
    seed_local_job(&runtime, job_id, "demo", dir.clone()).await;
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
    let dir = write_fake_job(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "running",
        "",
        "",
        json!({ "started_at": past, "max_runtime_secs": 60 }),
    );
    seed_local_job(&runtime, job_id, "demo", dir).await;
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
    let dir = write_fake_job_with_pgid(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "running",
        4242,
        json!({ "started_at": past, "max_runtime_secs": 60, "process_group_id": 4242 }),
    );
    seed_local_job(&runtime, job_id, "demo", dir).await;
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
    let dir = write_fake_job_with_pgid(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "completed",
        9999,
        json!({ "started_at": past, "max_runtime_secs": 60, "process_group_id": 9999 }),
    );
    seed_local_job(&runtime, job_id, "demo", dir).await;
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
    seed_local_job(&runtime, job_id, "demo", dir.clone()).await;
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
    let dir = write_fake_job_with_pgid(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "completed",
        8888,
        json!({ "started_at": past, "max_runtime_secs": 60, "process_group_id": 8888 }),
    );
    seed_local_job(&runtime, job_id, "demo", dir).await;
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
    seed_local_job(&runtime, job_id, "demo", dir.clone()).await;

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
    assert_eq!(result.output["stop_effect"], "confirmation_required");
    assert_eq!(result.output["stop_request_accepted"], false);
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
    seed_local_job(&runtime, job_id, "demo", dir.clone()).await;

    let result = runtime
        .dispatch(ToolCall::StopJob {
            project: "demo".to_string(),
            job_id: job_id.to_string(),
            session_id: Some(session.session_id.clone()),
            confirm: true,
        })
        .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["already_finished"], false);
    assert_eq!(result.output["already_stop_requested"], false);
    assert_eq!(result.output["stop_request_accepted"], true);
    assert_eq!(result.output["target_was_active_at_request"], true);
    assert_eq!(result.output["terminal"], true);
    assert_eq!(result.output["terminal_pending"], false);
    assert_eq!(result.output["final_status"], "stopped");
    assert_eq!(result.output["stop_effect"], "stopped");
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
    let dir = write_fake_job_with_pgid(
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
    seed_local_job(&runtime, job_id, "demo", dir).await;

    let result = runtime
        .dispatch(ToolCall::StopJob {
            project: "demo".to_string(),
            job_id: job_id.to_string(),
            session_id: Some(session.session_id.clone()),
            confirm: true,
        })
        .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["already_finished"], true);
    assert_eq!(result.output["already_stop_requested"], false);
    assert_eq!(result.output["stop_request_accepted"], false);
    assert_eq!(result.output["target_was_active_at_request"], false);
    assert_eq!(result.output["terminal"], true);
    assert_eq!(result.output["terminal_pending"], false);
    assert_eq!(result.output["final_status"], "completed");
    assert_eq!(result.output["stop_effect"], "already_finished");
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
    let dir = write_fake_job_with_pgid(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "running",
        1212,
        json!({ "started_at": now, "max_runtime_secs": 3600, "process_group_id": 1212 }),
    );
    seed_local_job(&runtime, job_id, "demo", dir).await;

    let result = runtime
        .dispatch(ToolCall::StopJob {
            project: "demo".to_string(),
            job_id: job_id.to_string(),
            session_id: None,
            confirm: true,
        })
        .await;

    assert!(result.success, "{:?}", result.error);
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
    let dir = write_fake_job_with_pgid(
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
    seed_local_job(&runtime, job_id, "demo", dir).await;

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
                purpose: None,
                shell: None,
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
async fn model_facing_stop_job_reports_requested_and_already_stop_requested() {
    let runtime = test_runtime();
    let auth = open_auth_context();
    register_job_agent_for_auth(&runtime, "client-stop-pending", "proj-stop-pending", &auth).await;
    let project = "agent:client-stop-pending:proj-stop-pending".to_string();
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("agent stop pending".to_string()),
    );
    let run = runtime
        .dispatch_with_auth(
            ToolCall::RunJob {
                project: project.clone(),
                command: "echo stop-pending".to_string(),
                session_id: Some(session.session_id.clone()),
                timeout_secs: None,
                cwd: None,
                purpose: None,
                shell: None,
            },
            Some(&auth),
        )
        .await;
    assert!(run.success, "{:?}", run.error);
    let job_id = run.output["job_id"].as_str().unwrap().to_string();
    let start_req = next_agent_request_for_instance(&runtime, "client-stop-pending", "inst")
        .await
        .expect("agent should receive start_job");
    assert_eq!(start_req.kind, "start_job");

    let result = runtime
        .dispatch_with_auth(
            ToolCall::StopJob {
                project: project.clone(),
                job_id: job_id.clone(),
                session_id: Some(session.session_id.clone()),
                confirm: true,
            },
            Some(&auth),
        )
        .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["already_finished"], false);
    assert_eq!(result.output["already_stop_requested"], false);
    assert_eq!(result.output["status_before"], "agent_queued");
    assert_eq!(result.output["status_after"], "stop_requested");
    assert_eq!(result.output["stop_request_accepted"], true);
    assert_eq!(result.output["target_was_active_at_request"], true);
    assert_eq!(result.output["terminal"], false);
    assert_eq!(result.output["terminal_pending"], true);
    assert!(result.output["final_status"].is_null());
    assert_eq!(result.output["stop_effect"], "requested");

    let status = runtime
        .dispatch_with_auth(
            ToolCall::JobStatus {
                job_id: job_id.clone(),
                include_command_preview: false,
            },
            Some(&auth),
        )
        .await;
    assert!(status.success, "{:?}", status.error);
    assert_eq!(status.output["status"], "stop_requested");
    assert_eq!(status.output["active"], true);
    assert_eq!(status.output["blocking_active"], false);
    assert_eq!(status.output["terminal"], false);
    assert_eq!(status.output["terminal_pending"], true);
    assert_eq!(status.output["command_preview_included"], false);

    let second = runtime
        .dispatch_with_auth(
            ToolCall::StopJob {
                project,
                job_id,
                session_id: Some(session.session_id),
                confirm: true,
            },
            Some(&auth),
        )
        .await;
    assert!(second.success, "{:?}", second.error);
    assert_eq!(second.output["already_finished"], false);
    assert_eq!(second.output["already_stop_requested"], true);
    assert_eq!(second.output["status_before"], "stop_requested");
    assert_eq!(second.output["status_after"], "stop_requested");
    assert_eq!(second.output["stop_request_accepted"], false);
    assert_eq!(second.output["target_was_active_at_request"], true);
    assert_eq!(second.output["terminal"], false);
    assert_eq!(second.output["terminal_pending"], true);
    assert!(second.output["final_status"].is_null());
    assert_eq!(second.output["stop_effect"], "already_stop_requested");
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
                window: None,
                record_oauth_scope_denials: true,
                host_file_import_trust: crate::tool_runtime::kernel::HostFileImportTrust::Untrusted,
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
async fn job_log_returns_bounded_output_for_in_memory_local_job() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let runtime = runtime_with_project(root, "demo");
    let job_id = "88888888-9999-0000-1111-222222222222";
    let stdout = (1..=1000)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    let dir = write_fake_job(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "running",
        &stdout,
        "",
        json!({}),
    );
    seed_local_job(&runtime, job_id, "demo", dir).await;
    let result = runtime.job_log(job_id.to_string(), None, None).await;
    assert!(result.success);
    let out = result.output["stdout_tail"].as_str().unwrap();
    let lines: Vec<&str> = out.lines().collect();
    assert!(lines.len() <= MAX_LOCAL_LOG_LINES);
    assert!(out.contains("line 1000"));
    assert!(!out.contains("line 1\n"));
    assert_eq!(result.output["status"], "lost");
    assert!(result.output["exit_code"].is_null());
    assert_eq!(result.output["stdout_lines"], 1000);
    assert_eq!(result.output["stdout_truncated"], true);
    assert_eq!(result.output["cursor"]["stdout"], 1001);
    assert_eq!(result.output["command_summary"], "echo test");
    assert_eq!(result.output["detected_summary"]["kind"], "operation");
    assert_eq!(result.output["detected_summary"]["outcome"], "failed");
}

#[tokio::test]
async fn job_log_paginates_with_offset_for_in_memory_local_job() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let runtime = runtime_with_project(root, "demo");
    let job_id = "99999999-0000-1111-2222-333333333333";
    let stdout = (1..=600)
        .map(|i| format!("line {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    let dir = write_fake_job(
        root,
        job_id,
        "demo",
        &root.to_string_lossy(),
        "running",
        &stdout,
        "",
        json!({}),
    );
    seed_local_job(&runtime, job_id, "demo", dir).await;
    let first = runtime.job_log(job_id.to_string(), Some(1), None).await;
    assert!(first.success);
    let out = first.output["stdout_tail"].as_str().unwrap();
    assert!(out.contains("line 1"));
    assert!(out.contains("line 500"));
    assert!(!out.contains("line 501"));
    assert_eq!(first.output["cursor"]["stdout"], 501);

    let second = runtime.job_log(job_id.to_string(), Some(501), None).await;
    assert!(second.success);
    let out2 = second.output["stdout_tail"].as_str().unwrap();
    assert!(out2.contains("line 501"));
    assert!(out2.contains("line 600"));
    assert_eq!(second.output["cursor"]["stdout"], 601);
}

#[tokio::test]
async fn inspect_job_log_and_tail_use_terminal_snapshot_global_cursors() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_project(tmp.path(), "demo");
    let scratch = crate::command_sandbox::InspectScratch::create().unwrap();
    let scratch_path = scratch.path().to_path_buf();
    let dir = scratch.path().join("job");
    fs::create_dir(&dir).unwrap();
    let stdout = (1..=700)
        .map(|line| format!("stdout {line}\n"))
        .collect::<String>();
    let stderr = (1..=700)
        .map(|line| format!("stderr {line}\n"))
        .collect::<String>();
    fs::write(
        dir.join("metadata.json"),
        serde_json::to_string(&json!({
            "command": "echo test",
            "cwd": ".",
            "shell": "bash",
            "purpose": "other"
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(dir.join("status"), "running").unwrap();
    fs::write(dir.join("stdout.log"), b"").unwrap();
    fs::write(dir.join("stderr.log"), b"").unwrap();

    let job_id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    let (record, _) = LocalJobRecord::initialize("demo".to_string(), dir.clone()).unwrap();
    fs::write(dir.join("stdout.log"), stdout).unwrap();
    fs::write(dir.join("stderr.log"), stderr).unwrap();
    fs::write(dir.join("status"), "completed").unwrap();
    fs::write(dir.join("exit_code"), "0").unwrap();
    fs::write(dir.join("finished_at"), "100").unwrap();
    let snapshot = record.terminal_snapshot_handle();
    runtime
        .local_jobs
        .lock()
        .await
        .insert(job_id.to_string(), record);
    let child = std::process::Command::new("sh")
        .arg("-c")
        .arg("exit 0")
        .spawn()
        .unwrap();
    super::super::local_jobs::retain_inspect_job_until_terminal(dir, snapshot, scratch, child);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while scratch_path.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "inspect scratch cleanup timed out"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let from_start = runtime.job_log(job_id.to_string(), Some(1), None).await;
    assert!(from_start.success, "{:?}", from_start.error);
    assert_eq!(from_start.output["stdout_lines"], 700);
    assert_eq!(from_start.output["stderr_lines"], 700);
    assert_eq!(from_start.output["stdout_truncated"], true);
    assert_eq!(from_start.output["stderr_truncated"], true);
    assert_eq!(from_start.output["cursor"]["stdout"], 701);
    assert_eq!(from_start.output["cursor"]["stderr"], 701);
    assert_eq!(
        from_start.output["stdout_tail"]
            .as_str()
            .unwrap()
            .lines()
            .next(),
        Some("stdout 201")
    );
    assert_eq!(
        from_start.output["stderr_tail"]
            .as_str()
            .unwrap()
            .lines()
            .next(),
        Some("stderr 201")
    );

    let tail = runtime.job_tail(job_id.to_string(), Some(5)).await;
    assert!(tail.success, "{:?}", tail.error);
    assert_eq!(tail.output["stdout_lines"], 700);
    assert_eq!(tail.output["stderr_lines"], 700);
    assert_eq!(tail.output["stdout_truncated"], true);
    assert_eq!(tail.output["stderr_truncated"], true);
    assert_eq!(tail.output["cursor"]["stdout"], 701);
    assert_eq!(tail.output["cursor"]["stderr"], 701);
    assert_eq!(
        tail.output["stdout_tail"].as_str().unwrap(),
        "stdout 696\nstdout 697\nstdout 698\nstdout 699\nstdout 700"
    );
    assert_eq!(
        tail.output["stderr_tail"].as_str().unwrap(),
        "stderr 696\nstderr 697\nstderr 698\nstderr 699\nstderr 700"
    );
    assert_eq!(tail.output["status"], "completed");
    assert_eq!(tail.output["terminal"], true);
    assert_eq!(tail.output["exit_code"], 0);
    let terminal_token = tail.output["observation_token"]
        .as_str()
        .unwrap()
        .to_string();
    let again = runtime
        .job_log_for_auth(
            job_id.to_string(),
            None,
            Some(5),
            None,
            Some(terminal_token.clone()),
            Some(1),
        )
        .await;
    assert!(again.success, "{:?}", again.error);
    assert_eq!(again.output["wait_outcome"], "terminal");
    assert_eq!(again.output["observation_token"], terminal_token);
    assert_eq!(again.output["status"], "completed");
    assert_eq!(again.output["exit_code"], 0);
    assert!(!scratch_path.exists());
}

async fn register_job_agent_for_auth(
    runtime: &ToolRuntime,
    client_id: &str,
    project_id: &str,
    auth: &crate::auth::AuthContext,
) {
    let caps = ShellClientCapabilities {
        async_shell_jobs: true,
        ..Default::default()
    };
    runtime
        .shell_clients
        .register_with_auth(
            ShellClientRegisterRequest {
                process_started_at: None,
                build: None,
                job_concurrency_limit: Some(4),
                job_inventory: None,
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
                purpose: None,
                shell: None,
            },
            Some(auth),
        )
        .await;
    assert!(result.success, "{:?}", result.error);
    result.output["job_id"].as_str().unwrap().to_string()
}

async fn mark_next_agent_job_running(runtime: &ToolRuntime, client_id: &str) -> String {
    let request = next_agent_request_for_instance(runtime, client_id, "inst")
        .await
        .expect("Agent Job request should be queued");
    let job_id = request.job_id.clone().expect("Job request id");
    runtime
        .shell_clients
        .update_job(ShellAgentJobUpdateRequest {
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            job_id: job_id.clone(),
            request_id: Some(request.request_id),
            update_seq: None,
            status: "running".to_string(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            command_execution_state: None,
            validation_progress: None,
            finished: false,
        })
        .await
        .unwrap();
    job_id
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
async fn shared_key_runtime_job_tools_filter_agent_jobs_by_auth_group() {
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
                    after_observation_token: None,
                    wait_secs: None,
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
                    after_observation_token: None,
                    wait_secs: None,
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
                after_observation_token: None,
                wait_secs: None,
            },
            Some(&shared_b),
        )
        .await;
    assert!(log_b.success, "{:?}", log_b.error);
    assert_eq!(log_b.output["stdout_tail"], "b-out\n");

    let tail_b = runtime
        .dispatch_with_auth(
            ToolCall::JobTail {
                job_id: job_b,
                tail_lines: Some(10),
                after_observation_token: None,
                wait_secs: None,
            },
            Some(&shared_b),
        )
        .await;
    assert!(tail_b.success, "{:?}", tail_b.error);
    assert_eq!(tail_b.output["stdout_tail"], "b-out\n");
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
    let record = LocalJobRecord::initialize("demo".to_string(), dir)
        .map(|(record, _)| record)
        .unwrap();
    runtime
        .local_jobs
        .lock()
        .await
        .insert("job-local".to_string(), record);
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
async fn runtime_status_and_list_agents_filter_concurrency_counts_by_auth_group() {
    let runtime = test_runtime();
    let shared_a = shared_key_auth_context("hash-a");
    let shared_b = shared_key_auth_context("hash-b");
    let open = open_auth_context();
    let bootstrap = bootstrap_auth_context();

    register_job_agent_for_auth(&runtime, "status-a", "proj-a", &shared_a).await;
    register_job_agent_for_auth(&runtime, "status-b", "proj-b", &shared_b).await;
    register_job_agent_for_auth(&runtime, "status-open", "proj-open", &open).await;

    let job_a_running = start_agent_runtime_job(&runtime, "status-a", "proj-a", &shared_a).await;
    assert_eq!(
        mark_next_agent_job_running(&runtime, "status-a").await,
        job_a_running
    );
    let _job_a_queued = start_agent_runtime_job(&runtime, "status-a", "proj-a", &shared_a).await;
    let job_b_running = start_agent_runtime_job(&runtime, "status-b", "proj-b", &shared_b).await;
    assert_eq!(
        mark_next_agent_job_running(&runtime, "status-b").await,
        job_b_running
    );
    let _job_b_queued = start_agent_runtime_job(&runtime, "status-b", "proj-b", &shared_b).await;
    let _job_open = start_agent_runtime_job(&runtime, "status-open", "proj-open", &open).await;

    let status_a = runtime
        .dispatch_with_auth(
            ToolCall::RuntimeStatus {
                compact: false,
                summary_only: false,
            },
            Some(&shared_a),
        )
        .await;
    assert!(status_a.success, "{:?}", status_a.error);
    assert_eq!(status_a.output["jobs"]["agent_known_count"], 2);
    assert_eq!(status_a.output["jobs"]["active_count"], 2);
    assert_eq!(status_a.output["jobs"]["running_count"], 1);
    assert_eq!(status_a.output["jobs"]["queued_count"], 1);
    assert_eq!(
        status_a.output["agents"]["clients"][0]["job_concurrency"],
        json!({"limit": 4, "running": 1, "queued": 1})
    );
    assert!(status_a.output["agents"]["clients"][0]
        .get("available_slots")
        .is_none());
    assert!(status_a.output["agents"]["clients"][0]
        .get("saturated")
        .is_none());

    let agents_a = runtime
        .dispatch_with_auth(ToolCall::ListAgents, Some(&shared_a))
        .await;
    assert!(agents_a.success, "{:?}", agents_a.error);
    assert_eq!(agents_a.output["count"], 1);
    assert_eq!(agents_a.output["agents"][0]["client_id"], "status-a");
    assert_eq!(
        agents_a.output["agents"][0]["job_concurrency"],
        json!({"limit": 4, "running": 1, "queued": 1})
    );
    assert_eq!(
        agents_a.output["clients"][0]["job_concurrency"],
        json!({"limit": 4, "running": 1, "queued": 1})
    );
    let new_observability = agents_a.output["agents"][0]["job_concurrency"]
        .as_object()
        .unwrap();
    assert_eq!(new_observability.len(), 3);
    for forbidden in [
        "stdout",
        "stderr",
        "script",
        "argv",
        "stdin",
        "command",
        "token",
        "credentials",
    ] {
        assert!(new_observability.get(forbidden).is_none(), "{forbidden}");
    }

    let status_open = runtime
        .dispatch_with_auth(
            ToolCall::RuntimeStatus {
                compact: false,
                summary_only: false,
            },
            Some(&open),
        )
        .await;
    assert!(status_open.success, "{:?}", status_open.error);
    assert_eq!(status_open.output["jobs"]["agent_known_count"], 1);
    assert_eq!(status_open.output["jobs"]["active_count"], 1);
    assert_eq!(status_open.output["jobs"]["running_count"], 0);
    assert_eq!(status_open.output["jobs"]["queued_count"], 1);

    let status_bootstrap = runtime
        .dispatch_with_auth(
            ToolCall::RuntimeStatus {
                compact: false,
                summary_only: false,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(status_bootstrap.success, "{:?}", status_bootstrap.error);
    assert_eq!(status_bootstrap.output["jobs"]["agent_known_count"], 5);
    assert_eq!(status_bootstrap.output["jobs"]["active_count"], 5);
    assert_eq!(status_bootstrap.output["jobs"]["running_count"], 2);
    assert_eq!(status_bootstrap.output["jobs"]["queued_count"], 3);
    assert_eq!(status_bootstrap.output["agents"]["count"], 3);

    let compact_a = runtime
        .dispatch_with_auth(
            ToolCall::RuntimeStatus {
                compact: true,
                summary_only: false,
            },
            Some(&shared_a),
        )
        .await;
    assert_eq!(
        compact_a.output["jobs"],
        json!({"active_count": 2, "running_count": 1, "queued_count": 1})
    );
}

#[tokio::test]
async fn runtime_concurrency_counts_cover_all_visible_jobs_beyond_list_pagination() {
    let runtime = test_runtime();
    let auth = bootstrap_auth_context();
    register_job_agent_for_auth(&runtime, "count-all", "proj-all", &auth).await;
    for _ in 0..21 {
        start_agent_runtime_job(&runtime, "count-all", "proj-all", &auth).await;
    }

    let status = runtime
        .dispatch_with_auth(
            ToolCall::RuntimeStatus {
                compact: false,
                summary_only: false,
            },
            Some(&auth),
        )
        .await;
    assert_eq!(status.output["jobs"]["agent_known_count"], 21);
    assert_eq!(status.output["jobs"]["active_count"], 21);
    assert_eq!(status.output["jobs"]["running_count"], 0);
    assert_eq!(status.output["jobs"]["queued_count"], 21);
    assert_eq!(
        status.output["agents"]["clients"][0]["job_concurrency"],
        json!({"limit": 4, "running": 0, "queued": 21})
    );
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
        LocalJobRecord::new("demo".to_string(), dir),
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
            LocalJobRecord::new("demo".to_string(), dir),
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
            after_observation_token: None,
            wait_secs: None,
        })
        .await;
    assert!(!result.success);
    assert!(
        result.error.unwrap().contains("unknown job"),
        "job_tail should report unknown job"
    );
}

// ============================================================================
// Bounded `job_log`/`job_tail` waits — observation tokens and local jobs
// ============================================================================

fn make_local_record(
    job_id: &str,
) -> (
    tempfile::TempDir,
    LocalJobRecord,
    super::super::local_jobs::LocalJobObservation,
) {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join(job_id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("status"), "running").unwrap();
    fs::write(dir.join("stdout.log"), b"").unwrap();
    fs::write(dir.join("stderr.log"), b"").unwrap();
    let now = chrono::Utc::now().timestamp();
    fs::write(dir.join("metadata.json"), serde_json::to_vec(&json!({"created_at":now,"started_at":now,"max_runtime_secs":3600,"kind":"shell","purpose":"test","command":"printf test","cwd":".","shell":"bash"})).unwrap()).unwrap();
    let (record, observation) = LocalJobRecord::initialize("project".to_string(), dir).unwrap();
    (temp, record, observation)
}

#[tokio::test]
async fn job_log_wait_rejects_invalid_wait_secs_before_execution() {
    let runtime = test_runtime();
    for invalid in [0u64, 61u64] {
        let result = runtime
            .dispatch_with_auth(
                ToolCall::JobLog {
                    job_id: "11111111-2222-3333-4444-555555555555".to_string(),
                    offset: None,
                    tail_lines: None,
                    after_observation_token: Some("bad".to_string()),
                    wait_secs: Some(invalid),
                },
                None,
            )
            .await;
        assert!(!result.success);
        assert!(result.error.as_deref().unwrap_or("").contains("wait_secs"));
    }
}

#[test]
fn local_noop_observation_does_not_rewrite_file() {
    let (_temp, record, initial) = make_local_record("job-noop");
    #[cfg(unix)]
    let path = record.dir.join("observation.json");
    #[cfg(unix)]
    let before = fs::metadata(&path).unwrap();
    let first = record.observe().unwrap();
    let second = record.observe().unwrap();
    #[cfg(unix)]
    let after = fs::metadata(&path).unwrap();
    assert_eq!(initial, first);
    assert_eq!(first, second);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(before.ino(), after.ino());
        assert_eq!(before.mtime_nsec(), after.mtime_nsec());
    }
}

#[tokio::test]
async fn local_log_growth_changes_token_and_freezes_snapshot_length() {
    let (_temp, record, initial) = make_local_record("job-growth");
    fs::write(record.dir.join("stdout.log"), b"one\n").unwrap();
    let observed = record.observe().unwrap();
    assert_eq!(observed.revision, initial.revision + 1);
    let token0 = initial.token("job-growth").unwrap();
    let result = super::super::jobs::local_job_log(
        "job-growth",
        &record,
        &SystemJobKiller,
        None,
        None,
        Some(token0),
        Some(5),
    )
    .await;
    assert!(result.success);
    assert_eq!(result.output["wait_outcome"], "immediate");
    assert_eq!(result.output["changed"], true);
    assert_eq!(result.output["stdout_tail"], "one");
    assert_ne!(
        result.output["observation_token"],
        initial.token("job-growth").unwrap()
    );
}

#[test]
fn local_reconstructed_record_keeps_epoch_and_revision() {
    let (_temp, record, _) = make_local_record("job-reconstruct");
    fs::write(record.dir.join("stdout.log"), b"one\n").unwrap();
    let advanced = record.observe().unwrap();
    let reconstructed = LocalJobRecord::new("project".to_string(), record.dir.clone());
    let observed = reconstructed.observe().unwrap();
    assert_eq!(observed.epoch, advanced.epoch);
    assert_eq!(observed.revision, advanced.revision);
}

#[test]
fn local_persistence_failure_does_not_publish_new_revision() {
    let (_temp, record, initial) = make_local_record("job-persist-fail");
    fs::write(record.dir.join("stdout.log"), b"change\n").unwrap();
    let observation_path = record.dir.join("observation.json");
    fs::remove_file(&observation_path).unwrap();
    fs::create_dir(&observation_path).unwrap();
    let error = record.observe().unwrap_err();
    assert!(error.contains("persist_failed"));
    fs::remove_dir(&observation_path).unwrap();
    fs::write(&observation_path, serde_json::to_vec(&initial).unwrap()).unwrap();
    let recovered = LocalJobRecord::new("project".to_string(), record.dir.clone());
    assert_eq!(recovered.observe().unwrap().revision, initial.revision + 1);
}

#[test]
fn job_log_parses_opaque_observation_token_and_rejects_non_string_values() {
    let token = crate::job_observation::JobObservationToken::new(
        crate::job_observation::JobObservationExecutor::Local,
        "abc",
        "0123456789abcdef",
        7,
    )
    .unwrap()
    .encode();
    let parsed = ToolCall::from_tool_name(
        "job_log",
        json!({"job_id": "abc", "after_observation_token": token, "wait_secs": 5}),
    )
    .unwrap();
    match parsed {
        ToolCall::JobLog {
            after_observation_token,
            wait_secs,
            ..
        } => {
            assert_eq!(after_observation_token.as_deref(), Some(token.as_str()));
            assert_eq!(wait_secs, Some(5));
        }
        other => panic!("expected JobLog, got {other:?}"),
    }

    let result = ToolCall::from_tool_name(
        "job_log",
        json!({"job_id": "abc", "after_observation_token": 1, "wait_secs": 5}),
    );
    assert!(result.is_err());
}

#[tokio::test]
async fn local_fast_output_after_initial_handoff_is_immediately_visible() {
    let (_temp, record, initial) = make_local_record("job-fast-output");
    let token0 = initial.token("job-fast-output").unwrap();
    let stdout = record.dir.join("stdout.log");
    let status = record.dir.join("status");
    let exit_code = record.dir.join("exit_code");
    let command = format!(
        "printf 'fast output\\n' > {}; printf 0 > {}; printf completed > {}",
        shell_escape_simple(&stdout.to_string_lossy()),
        shell_escape_simple(&exit_code.to_string_lossy()),
        shell_escape_simple(&status.to_string_lossy()),
    );
    let child = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .spawn()
        .unwrap();
    child.wait_with_output().unwrap();

    let result = super::super::jobs::local_job_log(
        "job-fast-output",
        &record,
        &SystemJobKiller,
        None,
        None,
        Some(token0.clone()),
        Some(5),
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["wait_outcome"], "immediate");
    assert_eq!(result.output["changed"], true);
    assert_eq!(result.output["terminal"], true);
    assert_eq!(result.output["stdout_tail"], "fast output");
    assert_ne!(result.output["observation_token"], token0);
}

#[test]
fn local_status_and_terminal_transitions_advance_once_without_logs() {
    let (_temp, record, initial) = make_local_record("job-status-only");
    fs::write(record.dir.join("status"), "stop_requested").unwrap();
    let stopped = record.observe().unwrap();
    assert_eq!(stopped.revision, initial.revision + 1);
    assert_eq!(record.observe().unwrap().revision, stopped.revision);

    fs::write(record.dir.join("status"), "completed").unwrap();
    fs::write(record.dir.join("exit_code"), "0").unwrap();
    let terminal = record.observe().unwrap();
    assert_eq!(terminal.revision, stopped.revision + 1);
    assert!(terminal.terminal());
    assert_eq!(terminal.stdout_len, 0);
    assert_eq!(terminal.stderr_len, 0);
}

#[test]
fn local_concurrent_observers_publish_one_revision() {
    let (_temp, record, initial) = make_local_record("job-concurrent-observe");
    fs::write(record.dir.join("stdout.log"), b"one\n").unwrap();
    let record = std::sync::Arc::new(record);
    let threads = (0..8)
        .map(|_| {
            let record = record.clone();
            std::thread::spawn(move || record.observe().unwrap().revision)
        })
        .collect::<Vec<_>>();
    let revisions = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect::<Vec<_>>();
    assert!(revisions
        .iter()
        .all(|revision| *revision == initial.revision + 1));
}

#[test]
fn local_fixed_length_snapshot_defers_append_to_next_observation() {
    let (_temp, record, initial) = make_local_record("job-fixed-length");
    fs::write(record.dir.join("stdout.log"), b"one\n").unwrap();
    let first = record.observe().unwrap();
    assert_eq!(first.revision, initial.revision + 1);
    fs::OpenOptions::new()
        .append(true)
        .open(record.dir.join("stdout.log"))
        .unwrap()
        .write_all(b"two\n")
        .unwrap();
    let frozen = record.read_log_lines_at("stdout.log", None, None, first.stdout_len);
    assert_eq!(frozen.0, "one");
    let second = record.observe().unwrap();
    assert_eq!(second.revision, first.revision + 1);
    assert!(second.stdout_len > first.stdout_len);
    assert_ne!(
        first.token("job-fixed-length").unwrap(),
        second.token("job-fixed-length").unwrap()
    );
}

#[tokio::test]
async fn local_append_during_read_is_visible_on_next_call_and_excluded_from_waited_ms() {
    let (_temp, record, _) = make_local_record("job-append-during-read");
    let stdout_path = record.dir.join("stdout.log");
    fs::write(&stdout_path, b"one\n").unwrap();
    let first = record.observe().unwrap();
    let token1 = first.token("job-append-during-read").unwrap();

    super::super::local_jobs::reset_bounded_log_read_count(&stdout_path);
    super::super::local_jobs::set_bounded_log_read_delay(
        &stdout_path,
        Some(std::time::Duration::from_millis(250)),
    );
    let started = std::time::Instant::now();
    let task = tokio::spawn({
        let record = record.clone();
        let token1 = token1.clone();
        async move {
            let killer = SystemJobKiller;
            super::super::jobs::local_job_log(
                "job-append-during-read",
                &record,
                &killer,
                None,
                None,
                Some(token1),
                None,
            )
            .await
        }
    });

    let read_started_deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while super::super::local_jobs::bounded_log_read_count(&stdout_path) == 0 {
        assert!(
            std::time::Instant::now() < read_started_deadline,
            "bounded stdout scan did not start"
        );
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    fs::OpenOptions::new()
        .append(true)
        .open(&stdout_path)
        .unwrap()
        .write_all(b"two\n")
        .unwrap();
    fs::write(record.dir.join("exit_code"), "0").unwrap();
    fs::write(record.dir.join("status"), "completed").unwrap();

    let first_response = task.await.unwrap();
    super::super::local_jobs::set_bounded_log_read_delay(&stdout_path, None);
    assert!(first_response.success, "{:?}", first_response.error);
    assert!(started.elapsed() >= std::time::Duration::from_millis(200));
    assert_eq!(first_response.output["wait_outcome"], "immediate");
    assert_eq!(first_response.output["waited_ms"], 0);
    assert_eq!(first_response.output["changed"], false);
    assert_eq!(first_response.output["terminal"], false);
    assert_eq!(first_response.output["status"], "running");
    assert!(first_response.output["exit_code"].is_null());
    assert_eq!(first_response.output["stdout_tail"], "one");
    assert_eq!(first_response.output["observation_token"], token1);

    let second_response = super::super::jobs::local_job_log(
        "job-append-during-read",
        &record,
        &SystemJobKiller,
        None,
        None,
        Some(token1.clone()),
        Some(5),
    )
    .await;
    assert!(second_response.success, "{:?}", second_response.error);
    assert_eq!(second_response.output["wait_outcome"], "immediate");
    assert_eq!(second_response.output["changed"], true);
    assert_eq!(second_response.output["terminal"], true);
    assert_eq!(second_response.output["status"], "completed");
    assert_eq!(second_response.output["exit_code"], 0);
    assert_eq!(second_response.output["stdout_tail"], "one\ntwo");
    assert_ne!(second_response.output["observation_token"], token1);
}

#[tokio::test]
async fn local_runtime_deadline_crossing_preempts_longer_update_wait() {
    let (_temp, record, initial) = make_local_record("job-runtime-deadline");
    let started_at = chrono::Utc::now().timestamp();
    fs::write(
        record.dir.join("metadata.json"),
        serde_json::to_vec(&json!({
            "created_at": started_at,
            "started_at": started_at,
            "max_runtime_secs": 2,
            "process_group_id": 4242,
            "kind": "shell",
            "purpose": "test",
            "command": "sleep 30",
            "cwd": ".",
            "shell": "bash"
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(record.dir.join("pid"), "4242").unwrap();
    let token0 = initial.token("job-runtime-deadline").unwrap();
    let killer = FakeJobKiller::default();
    let started = std::time::Instant::now();

    let response = super::super::jobs::local_job_log(
        "job-runtime-deadline",
        &record,
        &killer,
        None,
        None,
        Some(token0.clone()),
        Some(5),
    )
    .await;
    let elapsed = started.elapsed();

    assert!(response.success, "{:?}", response.error);
    assert!(
        elapsed < std::time::Duration::from_secs(4),
        "elapsed={elapsed:?}"
    );
    assert_eq!(response.output["wait_outcome"], "terminal");
    assert_eq!(response.output["changed"], true);
    assert_eq!(response.output["terminal"], true);
    assert_eq!(response.output["status"], "lost");
    assert_ne!(response.output["observation_token"], token0);
    assert_eq!(killer.calls(), vec![(4242, 4242)]);
}

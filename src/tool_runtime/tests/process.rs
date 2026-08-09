//! Model-facing structured process execution contract.

use super::super::*;
use super::support::*;
use crate::shell_protocol::{
    ShellAgentJobUpdateRequest, ShellAgentResultPayload, ShellAgentResultRequest,
    ShellClientCapabilities, ShellCommandExecutionState,
};
use crate::tool_runtime::kernel::{ToolCallContext, ToolCallRequest, ToolTransport};
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

struct ProcessArgvHelper {
    _temp: tempfile::TempDir,
    path: PathBuf,
}

static PROCESS_ARGV_HELPER: OnceLock<Arc<ProcessArgvHelper>> = OnceLock::new();

fn process_argv_helper() -> PathBuf {
    PROCESS_ARGV_HELPER
        .get_or_init(|| {
            let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/process_argv_helper.rs");
            let temp = tempfile::tempdir().unwrap();
            let output = temp.path().join(format!(
                "process-argv-helper{}",
                std::env::consts::EXE_SUFFIX
            ));
            let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
            let result = std::process::Command::new(rustc)
                .arg("--edition=2021")
                .arg("--crate-name=webcodex_process_argv_helper")
                .arg(source)
                .arg("-o")
                .arg(&output)
                .output()
                .expect("run rustc for process argv helper");
            assert!(
                result.status.success(),
                "process argv helper compilation failed: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            Arc::new(ProcessArgvHelper {
                _temp: temp,
                path: output,
            })
        })
        .path
        .clone()
}

fn process_call(project: String, session_id: Option<String>) -> ToolCall {
    ToolCall::RunProcess {
        project,
        executable: "argv-helper".to_string(),
        args: vec![
            "two words".to_string(),
            "$(literal)".to_string(),
            "雪".to_string(),
        ],
        stdin: Some("input\n".to_string()),
        session_id,
        timeout_secs: Some(30),
        cwd: None,
        purpose: Some(ExecutionPurpose::Diagnostic),
    }
}

async fn register_process_agent(
    runtime: &ToolRuntime,
    client_id: &str,
    root: &std::path::Path,
    structured_process_argv: bool,
    sandbox_inspect_commands: bool,
) -> String {
    let mut capabilities = ShellClientCapabilities::default();
    capabilities.shell = true;
    capabilities.structured_validation_argv = true;
    capabilities.structured_process_argv = structured_process_argv;
    capabilities.sandbox_inspect_commands = sandbox_inspect_commands;
    register_agent_with_projects(
        runtime,
        client_id,
        None,
        capabilities,
        vec![registered_project("demo", &root.to_string_lossy())],
    )
    .await;
    crate::tool_runtime::agent_project_runtime_id(client_id, "demo")
}

async fn register_process_job_agent(
    runtime: &ToolRuntime,
    client_id: &str,
    root: &std::path::Path,
) -> String {
    let mut capabilities = ShellClientCapabilities::default();
    capabilities.shell = true;
    capabilities.async_jobs = true;
    capabilities.async_shell_jobs = true;
    capabilities.structured_validation_argv = true;
    capabilities.structured_process_argv = true;
    capabilities.structured_script_payload = true;
    capabilities.structured_execution_jobs = true;
    register_agent_with_projects(
        runtime,
        client_id,
        None,
        capabilities,
        vec![registered_project("demo", &root.to_string_lossy())],
    )
    .await;
    crate::tool_runtime::agent_project_runtime_id(client_id, "demo")
}

async fn update_process_job(
    runtime: &ToolRuntime,
    client_id: &str,
    request: &crate::shell_protocol::ShellAgentShellRequest,
    status: &str,
    state: Option<ShellCommandExecutionState>,
    exit_code: Option<i32>,
    stdout: Option<&str>,
    stderr: Option<&str>,
    error: Option<&str>,
) {
    runtime
        .shell_clients
        .update_job(ShellAgentJobUpdateRequest {
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            job_id: request.job_id.clone().expect("structured Job id"),
            request_id: Some(request.request_id.clone()),
            update_seq: None,
            status: status.to_string(),
            stdout_chunk: stdout.map(str::to_string),
            stderr_chunk: stderr.map(str::to_string),
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code,
            duration_ms: state.map(|_| 25),
            error: error.map(str::to_string),
            command_execution_state: state,
            validation_progress: None,
            finished: state.is_some(),
        })
        .await
        .unwrap();
}

async fn complete_process_lifecycle(
    runtime: &ToolRuntime,
    client_id: &str,
    request_id: String,
    state: ShellCommandExecutionState,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
    error: Option<&str>,
) {
    runtime
        .shell_clients
        .complete(ShellAgentResultPayload {
            result: ShellAgentResultRequest {
                client_id: client_id.to_string(),
                agent_instance_id: "inst".to_string(),
                request_id,
                exit_code,
                stdout: Some(stdout.to_string()),
                stderr: Some(stderr.to_string()),
                duration_ms: Some(7),
                error: error.map(str::to_string),
            },
            command_execution_state: Some(state),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn run_process_local_direct_executor_preserves_argv_and_stdin_without_a_shell() {
    let cwd = tempfile::tempdir().unwrap();
    let marker = cwd.path().join("marker");
    let values = vec![
        String::new(),
        "two words".to_string(),
        "\"quotes\"".to_string(),
        "$(touch marker)".to_string(),
        "; touch marker".to_string(),
        "a&b|c".to_string(),
        r"C:\path with spaces\trailing\\".to_string(),
        "雪だるま☃".to_string(),
    ];
    let mut args = vec!["argv".to_string()];
    args.extend(values.clone());
    let (exit_code, stdout, stderr, _) =
        super::super::helpers::run_process_sync_bounded_with_sandbox(
            process_argv_helper().to_string_lossy().into_owned(),
            args,
            None,
            cwd.path().to_path_buf(),
            10,
            None,
        )
        .await
        .unwrap_or_else(|_| panic!("local direct argv helper should complete"));
    assert_eq!(exit_code, 0);
    assert_eq!(stderr, "");
    assert!(!marker.exists());
    let expected = values
        .iter()
        .map(|value| format!("{}:{value}\n", value.len()))
        .collect::<String>();
    assert_eq!(stdout, expected);

    let stdin = "line one\nUnicode 雪\n";
    let (exit_code, stdout, stderr, _) =
        super::super::helpers::run_process_sync_bounded_with_sandbox(
            process_argv_helper().to_string_lossy().into_owned(),
            vec!["stdin".to_string()],
            Some(stdin.to_string()),
            cwd.path().to_path_buf(),
            10,
            None,
        )
        .await
        .unwrap_or_else(|_| panic!("local direct stdin helper should complete"));
    assert_eq!(exit_code, 0);
    assert_eq!(stdout, stdin);
    assert_eq!(stderr, "");
}

#[tokio::test]
async fn run_process_enqueues_only_typed_argv_and_reports_completed_exit_codes() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project = register_process_agent(&runtime, "process-agent", temp.path(), true, false).await;
    let bootstrap = auth_context(None, true);

    for (exit_code, expected_success) in [(0, true), (19, false)] {
        let task = tokio::spawn({
            let runtime = runtime.clone();
            let project = project.clone();
            let bootstrap = bootstrap.clone();
            async move {
                runtime
                    .dispatch_with_auth(process_call(project, None), Some(&bootstrap))
                    .await
            }
        });
        let request = next_patch_agent_request(&runtime, "process-agent")
            .await
            .expect("run_process should enqueue");
        assert_eq!(request.kind, "run_process");
        assert_eq!(request.command, "");
        let process = request.process.as_ref().expect("typed process payload");
        assert_eq!(process.executable, "argv-helper");
        assert_eq!(
            process.args,
            ["two words", "$(literal)", "雪"].map(str::to_string)
        );
        assert_eq!(request.stdin.as_deref(), Some("input\n"));

        complete_process_lifecycle(
            &runtime,
            "process-agent",
            request.request_id,
            ShellCommandExecutionState::Completed,
            Some(exit_code),
            "stdout",
            "stderr",
            None,
        )
        .await;
        let result = task.await.unwrap();
        assert_eq!(result.success, expected_success);
        assert_eq!(result.output["execution_state"], "completed");
        assert_eq!(result.output["command_started"], true);
        assert_eq!(result.output["command_completed"], true);
        assert_eq!(result.output["exit_code"], exit_code);
        assert_eq!(result.output["execution_source"], "run_process");
        assert_eq!(result.output["purpose"], "diagnostic");
    }
}

#[tokio::test]
async fn run_process_fast_terminal_jobs_project_back_without_visible_duplicates() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime().with_structured_execution_sync_wait(Duration::from_millis(250));
    let project = register_process_job_agent(&runtime, "process-fast-job", temp.path()).await;
    let auth = auth_context(None, true);

    for (exit_code, status, expected_success) in [(0, "completed", true), (19, "failed", false)] {
        let task = tokio::spawn({
            let runtime = runtime.clone();
            let project = project.clone();
            let auth = auth.clone();
            async move {
                runtime
                    .dispatch_with_auth(process_call(project, None), Some(&auth))
                    .await
            }
        });
        let request = next_patch_agent_request(&runtime, "process-fast-job")
            .await
            .expect("hidden process Job should dispatch");
        assert_eq!(request.kind, "start_process_job");
        assert_eq!(request.command, "");
        assert!(request.process.is_some());
        assert!(request.script.is_none());
        update_process_job(
            &runtime,
            "process-fast-job",
            &request,
            "running",
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        update_process_job(
            &runtime,
            "process-fast-job",
            &request,
            status,
            Some(ShellCommandExecutionState::Completed),
            Some(exit_code),
            Some("terminal stdout\n"),
            Some("terminal stderr\n"),
            None,
        )
        .await;

        let result = task.await.unwrap();
        assert_eq!(result.success, expected_success);
        assert_eq!(result.output["execution_state"], "completed");
        assert_eq!(result.output["command_started"], true);
        assert_eq!(result.output["command_completed"], true);
        assert_eq!(result.output["exit_code"], exit_code);
        assert_eq!(result.output["promoted_to_job"], false);
        assert_eq!(result.output["terminal"], true);
        assert!(result.output["job_id"].is_null());
        assert!(result.output["job_status"].is_null());
        assert_eq!(result.output["async_handoff_available"], true);
        assert!(
            runtime
                .shell_clients
                .hidden_job_ids_for_test()
                .await
                .is_empty(),
            "a fast terminal execution must not retain its internal Job"
        );
        assert!(
            runtime.shell_clients.list_jobs(Some(10)).await.is_empty(),
            "a fast terminal execution must not become a public duplicate"
        );
    }
}

#[tokio::test]
async fn run_process_fast_terminal_projection_does_not_silently_drop_retained_lines() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime().with_structured_execution_sync_wait(Duration::from_millis(250));
    let project = register_process_job_agent(&runtime, "process-fast-log-job", temp.path()).await;
    let auth = auth_context(None, true);
    let retained_stdout = (0..300)
        .map(|line| format!("retained-line-{line:03}\n"))
        .collect::<String>();
    assert!(
        retained_stdout.len() < crate::shell_protocol::JOB_SNAPSHOT_STREAM_MAX_BYTES,
        "fixture must fit the Runner-retained snapshot bound"
    );
    assert!(
        retained_stdout.chars().count() < super::super::helpers::COMMAND_STDIO_TAIL_CHARS,
        "fixture must fit the model-facing terminal projection bound"
    );

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(process_call(project, None), Some(&auth))
                .await
        }
    });
    let request = next_patch_agent_request(&runtime, "process-fast-log-job")
        .await
        .expect("hidden process Job should dispatch");
    update_process_job(
        &runtime,
        "process-fast-log-job",
        &request,
        "running",
        None,
        None,
        None,
        None,
        None,
    )
    .await;
    update_process_job(
        &runtime,
        "process-fast-log-job",
        &request,
        "completed",
        Some(ShellCommandExecutionState::Completed),
        Some(0),
        Some(retained_stdout.as_str()),
        None,
        None,
    )
    .await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(
        result.output["stdout_tail"].as_str(),
        Some(retained_stdout.as_str())
    );
    assert_eq!(result.output["stdout_lines"], 300);
    assert_eq!(result.output["stdout_truncated"], false);
    assert_eq!(result.output["promoted_to_job"], false);
    assert!(runtime.shell_clients.list_jobs(Some(10)).await.is_empty());
}

#[tokio::test]
async fn run_process_fast_prestart_rejection_retains_not_started_through_the_hidden_job() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime().with_structured_execution_sync_wait(Duration::from_millis(250));
    let project = register_process_job_agent(&runtime, "process-prestart-job", temp.path()).await;
    let auth = auth_context(None, true);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(process_call(project, None), Some(&auth))
                .await
        }
    });
    let request = next_patch_agent_request(&runtime, "process-prestart-job")
        .await
        .expect("hidden process Job should dispatch");
    let queued = runtime
        .shell_clients
        .get_hidden_job_for_auth(Some(&auth), request.job_id.as_deref().unwrap())
        .await
        .unwrap();
    assert_eq!(queued.status, "agent_queued");
    assert_eq!(
        queued.started_at, None,
        "Runner request dispatch alone must not imply child spawn"
    );
    update_process_job(
        &runtime,
        "process-prestart-job",
        &request,
        "failed",
        Some(ShellCommandExecutionState::NotStarted),
        None,
        None,
        None,
        Some("failed to spawn structured process: executable not found"),
    )
    .await;

    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "not_started");
    assert_eq!(result.output["command_started"], false);
    assert_eq!(result.output["command_completed"], false);
    assert_eq!(result.output["promoted_to_job"], false);
    assert_eq!(result.output["terminal"], true);
    assert_eq!(result.output["failure_kind"], "spawn_failed");
    assert!(runtime.shell_clients.list_jobs(Some(10)).await.is_empty());
}

#[tokio::test]
async fn run_process_slow_handoff_is_queryable_once_and_keeps_the_original_budget() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime().with_structured_execution_sync_wait(Duration::from_millis(40));
    let project = register_process_job_agent(&runtime, "process-slow-job", temp.path()).await;
    let auth = auth_context(None, true);
    let started = Instant::now();
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::RunProcess {
                        project,
                        executable: "argv-helper".to_string(),
                        args: vec![
                            "two words".to_string(),
                            "$(literal)".to_string(),
                            "雪".to_string(),
                        ],
                        stdin: Some("input\n".to_string()),
                        session_id: None,
                        timeout_secs: Some(121),
                        cwd: None,
                        purpose: Some(ExecutionPurpose::Diagnostic),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = next_patch_agent_request(&runtime, "process-slow-job")
        .await
        .expect("typed process Job should dispatch");
    assert_eq!(request.kind, "start_process_job");
    assert_eq!(request.command, "");
    assert!(request.process.is_some());
    assert!(request.script.is_none());
    assert_eq!(request.timeout_secs, 121);
    update_process_job(
        &runtime,
        "process-slow-job",
        &request,
        "running",
        None,
        None,
        None,
        None,
        None,
    )
    .await;

    let handoff = task.await.unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(handoff.output["promoted_to_job"], true);
    assert_eq!(handoff.output["terminal"], false);
    assert_eq!(handoff.output["execution_state"], "running");
    assert_eq!(handoff.output["command_started"], true);
    assert_eq!(handoff.output["command_completed"], false);
    assert_eq!(handoff.output["effective_timeout_secs"], 121);
    assert_eq!(handoff.output["sync_wait_secs"], 10);
    let job_id = handoff.output["job_id"].as_str().unwrap().to_string();
    assert_eq!(request.job_id.as_deref(), Some(job_id.as_str()));

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
    assert_eq!(status.output["job_id"], job_id);
    assert_eq!(status.output["status"], "running");
    assert!(status.output["command_execution_state"].is_null());
    assert_eq!(
        status.output["structured_execution"]["execution_source"],
        "run_process"
    );
    assert_eq!(status.output["structured_execution"]["arg_count"], 3);
    assert_eq!(status.output["structured_execution"]["stdin_present"], true);
    let list = runtime
        .dispatch_with_auth(
            ToolCall::ListJobs {
                limit: Some(10),
                status: None,
            },
            Some(&auth),
        )
        .await;
    let listed = list.output["jobs"].as_array().unwrap();
    assert_eq!(
        listed.iter().filter(|job| job["job_id"] == job_id).count(),
        1
    );
    let listed_job = listed
        .iter()
        .find(|job| job["job_id"] == job_id)
        .expect("promoted process Job summary");
    assert_eq!(
        listed_job["structured_execution"]["execution_source"],
        "run_process"
    );
    assert!(next_patch_agent_request(&runtime, "process-slow-job")
        .await
        .is_none());

    update_process_job(
        &runtime,
        "process-slow-job",
        &request,
        "completed",
        Some(ShellCommandExecutionState::Completed),
        Some(0),
        Some("same execution complete\n"),
        None,
        None,
    )
    .await;
    let log = runtime
        .dispatch_with_auth(
            ToolCall::JobLog {
                job_id: job_id.clone(),
                offset: None,
                tail_lines: Some(20),
                after_observation_token: None,
                wait_secs: None,
            },
            Some(&auth),
        )
        .await;
    assert!(log.success, "{:?}", log.error);
    assert_eq!(log.output["status"], "completed");
    assert!(log.output["stdout_tail"]
        .as_str()
        .unwrap()
        .contains("same execution complete"));
    assert_eq!(log.output["command_execution_state"], "completed");
    assert_eq!(
        log.output["structured_execution"]["execution_source"],
        "run_process"
    );
    let observed = runtime
        .dispatch_with_auth(
            ToolCall::ObserveJobs {
                items: vec![ObserveJobsItem {
                    job_id: job_id.clone(),
                    after_observation_token: None,
                }],
                tail_lines: 20,
                wait_secs: None,
            },
            Some(&auth),
        )
        .await;
    assert!(observed.success, "{:?}", observed.error);
    let observed_job = &observed.output["items"][0]["output"];
    assert_eq!(observed_job["status"], log.output["status"]);
    assert_eq!(
        observed_job["command_execution_state"],
        log.output["command_execution_state"]
    );
    assert_eq!(
        observed_job["structured_execution"],
        log.output["structured_execution"]
    );
    assert_eq!(
        observed_job["observation_token"],
        log.output["observation_token"]
    );
    assert_eq!(
        runtime
            .shell_clients
            .get_job(&job_id)
            .await
            .unwrap()
            .command_execution_state,
        Some(ShellCommandExecutionState::Completed)
    );
    assert!(next_patch_agent_request(&runtime, "process-slow-job")
        .await
        .is_none());
}

#[tokio::test]
async fn stop_job_stops_the_promoted_process_without_starting_a_replacement() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime().with_structured_execution_sync_wait(Duration::from_millis(30));
    let project = register_process_job_agent(&runtime, "process-stop-job", temp.path()).await;
    let auth = auth_context(None, true);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(process_call(project, None), Some(&auth))
                .await
        }
    });
    let start_request = next_patch_agent_request(&runtime, "process-stop-job")
        .await
        .expect("structured process Job start");
    update_process_job(
        &runtime,
        "process-stop-job",
        &start_request,
        "running",
        None,
        None,
        None,
        None,
        None,
    )
    .await;
    let handoff = task.await.unwrap();
    let job_id = handoff.output["job_id"].as_str().unwrap().to_string();
    assert_eq!(handoff.output["promoted_to_job"], true);

    let stopped = runtime
        .dispatch_with_auth(
            ToolCall::StopJob {
                project,
                job_id: job_id.clone(),
                session_id: None,
                confirm: true,
            },
            Some(&auth),
        )
        .await;
    assert!(stopped.success, "{:?}", stopped.error);
    let stop_request = next_patch_agent_request(&runtime, "process-stop-job")
        .await
        .expect("existing stop_job API should dispatch");
    assert_eq!(stop_request.kind, "stop_job");
    assert_eq!(stop_request.job_id.as_deref(), Some(job_id.as_str()));
    update_process_job(
        &runtime,
        "process-stop-job",
        &start_request,
        "stopped",
        Some(ShellCommandExecutionState::Completed),
        Some(-1),
        None,
        Some("job stopped by request\n"),
        Some("job stopped"),
    )
    .await;
    let terminal = runtime.shell_clients.get_job(&job_id).await.unwrap();
    assert_eq!(terminal.status, "stopped");
    assert_eq!(
        terminal.command_execution_state,
        Some(ShellCommandExecutionState::Completed)
    );
    assert!(
        next_patch_agent_request(&runtime, "process-stop-job")
            .await
            .is_none(),
        "stopping the Job must never enqueue a replacement execution"
    );
}

#[tokio::test]
async fn promoted_process_inherits_the_initiating_session_without_a_second_tool_execution() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime().with_structured_execution_sync_wait(Duration::from_millis(30));
    let project = register_process_job_agent(&runtime, "process-session-job", temp.path()).await;
    let session = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        Some("structured process continuation".to_string()),
        SessionMode::Normal,
        sessions::SessionGuards::default(),
    );
    let auth = auth_context(None, true);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(process_call(project, Some(session_id)), Some(&auth))
                .await
        }
    });
    let request = next_patch_agent_request(&runtime, "process-session-job")
        .await
        .expect("Session-bound process Job");
    update_process_job(
        &runtime,
        "process-session-job",
        &request,
        "running",
        None,
        None,
        None,
        None,
        None,
    )
    .await;
    let handoff = task.await.unwrap();
    let job_id = handoff.output["job_id"].as_str().unwrap();
    let job = runtime.shell_clients.get_job(job_id).await.unwrap();
    assert_eq!(job.session_id.as_deref(), Some(session.session_id.as_str()));
    assert_eq!(job.project_id.as_deref(), Some(project.as_str()));
    assert_eq!(job.purpose.as_deref(), Some("diagnostic"));
    assert_eq!(job.kind, "run_process");

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(100))
        .unwrap();
    assert_eq!(
        summary
            .events
            .iter()
            .filter(|event| event.tool_name == "run_process" && event.kind == "tool_call_started")
            .count(),
        1,
        "handoff must not record a fake second model tool execution"
    );
    update_process_job(
        &runtime,
        "process-session-job",
        &request,
        "completed",
        Some(ShellCommandExecutionState::Completed),
        Some(0),
        None,
        None,
        None,
    )
    .await;
}

#[tokio::test]
async fn b2_process_runner_uses_direct_sync_and_rejects_durable_only_timeout() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let mut capabilities = ShellClientCapabilities::default();
    capabilities.shell = true;
    capabilities.async_jobs = true;
    capabilities.structured_process_argv = true;
    capabilities.structured_script_payload = true;
    capabilities.structured_execution_jobs = false;
    register_agent_with_projects(
        &runtime,
        "process-b2",
        None,
        capabilities,
        vec![registered_project("demo", &temp.path().to_string_lossy())],
    )
    .await;
    let project = crate::tool_runtime::agent_project_runtime_id("process-b2", "demo");
    let auth = auth_context(None, true);
    let direct = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::RunProcess {
                        project,
                        executable: "argv-helper".to_string(),
                        args: Vec::new(),
                        stdin: None,
                        session_id: None,
                        timeout_secs: Some(120),
                        cwd: None,
                        purpose: None,
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let request = next_patch_agent_request(&runtime, "process-b2")
        .await
        .expect("B2 direct request");
    assert_eq!(request.kind, "run_process");
    complete_process_lifecycle(
        &runtime,
        "process-b2",
        request.request_id,
        ShellCommandExecutionState::Completed,
        Some(0),
        "",
        "",
        None,
    )
    .await;
    let direct = direct.await.unwrap();
    assert_eq!(direct.output["promoted_to_job"], false);
    assert_eq!(direct.output["async_handoff_available"], false);

    let rejected = runtime
        .dispatch_with_auth(
            ToolCall::RunProcess {
                project,
                executable: "argv-helper".to_string(),
                args: Vec::new(),
                stdin: None,
                session_id: None,
                timeout_secs: Some(121),
                cwd: None,
                purpose: None,
            },
            Some(&auth),
        )
        .await;
    assert_eq!(rejected.output["execution_state"], "not_started");
    assert_eq!(rejected.output["command_started"], false);
    assert_eq!(rejected.output["failure_kind"], "capability_unavailable");
    assert!(next_patch_agent_request(&runtime, "process-b2")
        .await
        .is_none());
}

#[tokio::test]
async fn run_process_allows_typed_argv_larger_than_legacy_shell_command_limit() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project = register_process_agent(&runtime, "process-large", temp.path(), true, false).await;
    let large_args = vec!["a".repeat(4_500), "b".repeat(4_500)];
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let large_args = large_args.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::RunProcess {
                        project,
                        executable: "argv-helper".to_string(),
                        args: large_args,
                        stdin: None,
                        session_id: None,
                        timeout_secs: Some(30),
                        cwd: None,
                        purpose: None,
                    },
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });

    let request = next_patch_agent_request(&runtime, "process-large")
        .await
        .expect("large structured argv should enqueue");
    assert_eq!(request.command, "");
    let process = request.process.as_ref().unwrap();
    assert_eq!(process.args, large_args);
    assert!(process.args.iter().map(String::len).sum::<usize>() > 8_000);
    complete_process_lifecycle(
        &runtime,
        "process-large",
        request.request_id,
        ShellCommandExecutionState::Completed,
        Some(0),
        "",
        "",
        None,
    )
    .await;
    assert!(task.await.unwrap().success);
}

#[tokio::test]
async fn run_process_capability_absence_fails_prestart_without_shell_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project =
        register_process_agent(&runtime, "legacy-process-agent", temp.path(), false, false).await;

    let result = runtime
        .dispatch_with_auth(process_call(project, None), Some(&auth_context(None, true)))
        .await;

    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "not_started");
    assert_eq!(result.output["command_started"], false);
    assert_eq!(result.output["command_completed"], false);
    assert_eq!(result.output["failure_kind"], "capability_unavailable");
    assert!(result
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("no shell fallback"));
    assert!(
        next_patch_agent_request(&runtime, "legacy-process-agent")
            .await
            .is_none(),
        "capability failure must not enqueue run_process or run_shell"
    );
}

#[tokio::test]
async fn run_process_batch_rejection_from_runner_has_stable_prestart_contract() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project =
        register_process_agent(&runtime, "process-batch-rejected", temp.path(), true, false).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(process_call(project, None), Some(&auth_context(None, true)))
                .await
        }
    });
    let request = next_patch_agent_request(&runtime, "process-batch-rejected")
        .await
        .expect("run_process should reach the capable Runner");
    complete_process_lifecycle(
        &runtime,
        "process-batch-rejected",
        request.request_id,
        ShellCommandExecutionState::NotStarted,
        None,
        "",
        "",
        Some(
            "unsupported_executable_type: Windows .cmd/.bat files require shell/script semantics; use run_shell as the current explicit escape hatch",
        ),
    )
    .await;

    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "not_started");
    assert_eq!(result.output["command_started"], false);
    assert_eq!(result.output["command_completed"], false);
    assert_eq!(result.output["failure_kind"], "unsupported_executable_type");
    assert!(result
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("run_shell"));
}

#[tokio::test]
async fn authority_denied_run_process_has_prestart_lifecycle() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime().with_permission_evaluator(
        permissions::PermissionEvaluator::with_mode(permissions::AuthorityMode::Restricted),
    );
    let project =
        register_process_agent(&runtime, "process-authority", temp.path(), true, false).await;

    let result = runtime
        .dispatch_with_auth(process_call(project, None), Some(&auth_context(None, true)))
        .await;

    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "not_started");
    assert_eq!(result.output["command_started"], false);
    assert_eq!(result.output["command_completed"], false);
    assert_eq!(result.output["failure_kind"], "permission_denied");
    assert!(next_patch_agent_request(&runtime, "process-authority")
        .await
        .is_none());
}

#[tokio::test]
async fn run_process_transport_uncertainty_and_timeout_preserve_phase_a_truth() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project =
        register_process_agent(&runtime, "process-lifecycle", temp.path(), true, false).await;
    let uncertain_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_with_auth(process_call(project, None), Some(&auth_context(None, true)))
                .await
        }
    });
    next_patch_agent_request(&runtime, "process-lifecycle")
        .await
        .expect("run_process should dispatch before transport loss");
    runtime
        .shell_clients
        .reconcile_disconnect("process-lifecycle", "inst")
        .await;
    let uncertain = uncertain_task.await.unwrap();
    assert!(!uncertain.success);
    assert_eq!(uncertain.output["execution_state"], "outcome_unknown");
    assert_eq!(uncertain.output["command_started"], true);
    assert_eq!(uncertain.output["command_completed"], false);
    assert!(uncertain
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("Do not automatically retry"));

    let timeout_runtime = test_runtime();
    let timeout_project = register_process_agent(
        &timeout_runtime,
        "process-timeout",
        temp.path(),
        true,
        false,
    )
    .await;
    let timeout_task = tokio::spawn({
        let runtime = timeout_runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    process_call(timeout_project, None),
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    let request = next_patch_agent_request(&timeout_runtime, "process-timeout")
        .await
        .expect("run_process timeout should dispatch");
    complete_process_lifecycle(
        &timeout_runtime,
        "process-timeout",
        request.request_id,
        ShellCommandExecutionState::TimedOut,
        Some(-1),
        "",
        "process timed out",
        Some("process timed out"),
    )
    .await;
    let timed_out = timeout_task.await.unwrap();
    assert!(!timed_out.success);
    assert_eq!(timed_out.output["execution_state"], "timed_out");
    assert_eq!(timed_out.output["command_started"], true);
    assert_eq!(timed_out.output["command_completed"], false);
    assert!(timed_out
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("do not blindly retry"));
}

#[tokio::test]
async fn run_process_session_default_cwd_applies_without_default_shell() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let frontend = root.join("frontend");
    std::fs::create_dir_all(&frontend).unwrap();
    let runtime = test_runtime();
    let project = register_process_agent(&runtime, "process-context", &root, true, false).await;
    let session = runtime
        .sessions
        .start_session_with_options(
            sessions::SessionCreateOptions::new(
                Some(project.clone()),
                Some("process context".to_string()),
                SessionMode::Normal,
                sessions::SessionGuards::default(),
            )
            .with_execution_context(sessions::SessionExecutionContext {
                default_cwd: Some("frontend".to_string()),
                default_shell: Some(ExecutionShell::Bash),
                resource: None,
            }),
        )
        .unwrap();
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    process_call(project, Some(session_id)),
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });

    let request = next_patch_agent_request(&runtime, "process-context")
        .await
        .expect("session run_process should enqueue");
    assert_eq!(
        request.cwd.as_deref(),
        Some(frontend.to_string_lossy().as_ref())
    );
    assert_eq!(request.command, "");
    assert_eq!(request.process.as_ref().unwrap().executable, "argv-helper");
    complete_process_lifecycle(
        &runtime,
        "process-context",
        request.request_id,
        ShellCommandExecutionState::Completed,
        Some(0),
        "",
        "",
        None,
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["cwd"], "frontend");
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    let event = finished_event(&summary, "run_process");
    assert_eq!(event.risk_class, "job_run");
    assert!(event.shell_like);
    let started = summary
        .events
        .iter()
        .find(|event| event.kind == "tool_call_started" && event.tool_name == "run_process")
        .expect("run_process start event");
    let input = started
        .input_summary
        .as_ref()
        .expect("process input summary");
    assert_eq!(input["executable_present"], true);
    assert_eq!(input["arg_count"], 3);
    assert_eq!(input["stdin_present"], true);
    assert!(input.get("executable").is_none());
    assert!(input.get("args").is_none());
    assert!(input.get("stdin").is_none());
    assert!(input.get("process_summary").is_none());
}

#[tokio::test]
async fn run_process_named_ssh_resource_fails_before_enqueue() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project = register_process_agent(&runtime, "process-ssh", temp.path(), true, false).await;
    let session = runtime
        .sessions
        .start_session_with_options(
            sessions::SessionCreateOptions::new(
                Some(project.clone()),
                Some("remote process".to_string()),
                SessionMode::Normal,
                sessions::SessionGuards::default(),
            )
            .with_execution_context(sessions::SessionExecutionContext {
                default_cwd: Some("/srv/app".to_string()),
                default_shell: None,
                resource: Some("production".to_string()),
            }),
        )
        .unwrap();

    let result = runtime
        .dispatch_with_auth(
            process_call(project, Some(session.session_id)),
            Some(&auth_context(None, true)),
        )
        .await;

    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "not_started");
    assert_eq!(result.output["command_started"], false);
    assert_eq!(result.output["command_completed"], false);
    assert_eq!(result.output["failure_kind"], "unsupported_resource");
    assert!(next_patch_agent_request(&runtime, "process-ssh")
        .await
        .is_none());
}

#[tokio::test]
async fn run_process_validation_and_inspect_permission_boundaries_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir_all(&root).unwrap();
    let runtime = test_runtime();
    let project = register_process_agent(&runtime, "process-guards", &root, true, true).await;

    for invalid in [
        ToolCall::RunProcess {
            project: project.clone(),
            executable: String::new(),
            args: Vec::new(),
            stdin: None,
            session_id: None,
            timeout_secs: Some(30),
            cwd: None,
            purpose: None,
        },
        ToolCall::RunProcess {
            project: project.clone(),
            executable: "sh".to_string(),
            args: vec!["-c".to_string(), "touch marker".to_string()],
            stdin: None,
            session_id: None,
            timeout_secs: Some(30),
            cwd: None,
            purpose: None,
        },
        ToolCall::RunProcess {
            project: project.clone(),
            executable: "argv-helper".to_string(),
            args: vec!["bad\0arg".to_string()],
            stdin: None,
            session_id: None,
            timeout_secs: Some(30),
            cwd: None,
            purpose: None,
        },
        ToolCall::RunProcess {
            project: project.clone(),
            executable: "argv-helper".to_string(),
            args: vec![String::new(); 257],
            stdin: None,
            session_id: None,
            timeout_secs: Some(30),
            cwd: None,
            purpose: None,
        },
        ToolCall::RunProcess {
            project: project.clone(),
            executable: "argv-helper".to_string(),
            args: vec!["x".repeat(8_193)],
            stdin: None,
            session_id: None,
            timeout_secs: Some(30),
            cwd: None,
            purpose: None,
        },
        ToolCall::RunProcess {
            project: project.clone(),
            executable: "argv-helper".to_string(),
            args: Vec::new(),
            stdin: None,
            session_id: None,
            timeout_secs: Some(0),
            cwd: None,
            purpose: None,
        },
        ToolCall::RunProcess {
            project: project.clone(),
            executable: "argv-helper".to_string(),
            args: Vec::new(),
            stdin: None,
            session_id: None,
            timeout_secs: Some(30),
            cwd: Some("bad\0cwd".to_string()),
            purpose: None,
        },
        ToolCall::RunProcess {
            project: project.clone(),
            executable: "argv-helper".to_string(),
            args: Vec::new(),
            stdin: Some("x".repeat(65_537)),
            session_id: None,
            timeout_secs: Some(30),
            cwd: None,
            purpose: None,
        },
    ] {
        let result = runtime
            .dispatch_with_auth(invalid, Some(&auth_context(None, true)))
            .await;
        assert!(!result.success);
        assert_eq!(result.output["execution_state"], "not_started");
        assert_eq!(result.output["command_started"], false);
        assert_eq!(result.output["command_completed"], false);
        assert_eq!(result.output["failure_kind"], "invalid_arguments");
    }
    let durable_only = runtime
        .dispatch_with_auth(
            ToolCall::RunProcess {
                project: project.clone(),
                executable: "argv-helper".to_string(),
                args: Vec::new(),
                stdin: None,
                session_id: None,
                timeout_secs: Some(121),
                cwd: None,
                purpose: None,
            },
            Some(&auth_context(None, true)),
        )
        .await;
    assert_eq!(durable_only.output["execution_state"], "not_started");
    assert_eq!(durable_only.output["command_started"], false);
    assert_eq!(durable_only.output["command_completed"], false);
    assert_eq!(
        durable_only.output["failure_kind"],
        "capability_unavailable"
    );
    assert_eq!(durable_only.output["async_handoff_available"], false);
    assert!(!root.join("marker").exists());
    assert!(next_patch_agent_request(&runtime, "process-guards")
        .await
        .is_none());

    let inspect = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        Some("inspect process".to_string()),
        SessionMode::Inspect,
        sessions::SessionGuards::default(),
    );
    let inspect_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = inspect.session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    process_call(project, Some(session_id)),
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    let request = next_patch_agent_request(&runtime, "process-guards")
        .await
        .expect("inspect run_process should enqueue");
    assert_eq!(
        request.sandbox.as_deref(),
        Some(crate::command_sandbox::INSPECT_SANDBOX_MODE)
    );
    complete_process_lifecycle(
        &runtime,
        "process-guards",
        request.request_id,
        ShellCommandExecutionState::Completed,
        Some(0),
        "",
        "",
        None,
    )
    .await;
    assert!(inspect_task.await.unwrap().success);

    let read_only = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        Some("read-only process".to_string()),
        SessionMode::ReadOnly,
        sessions::SessionGuards::default(),
    );
    let denied = runtime
        .dispatch_with_auth(
            process_call(project, Some(read_only.session_id)),
            Some(&auth_context(None, true)),
        )
        .await;
    assert!(!denied.success);
    assert_eq!(denied.output["guard"], "deny_shell_tools");
    assert_eq!(denied.output["execution_state"], "not_started");
    assert_eq!(denied.output["command_started"], false);
    assert_eq!(denied.output["command_completed"], false);
    assert_eq!(denied.output["failure_kind"], "session_guard_denied");
    assert!(next_patch_agent_request(&runtime, "process-guards")
        .await
        .is_none());
}

#[tokio::test]
async fn closed_session_run_process_has_prestart_lifecycle() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project =
        register_process_agent(&runtime, "process-closed", temp.path(), true, false).await;
    let session = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        Some("closed process".to_string()),
        SessionMode::Normal,
        sessions::SessionGuards::default(),
    );
    runtime.sessions.close_session(&session.session_id).unwrap();

    let result = runtime
        .dispatch_with_auth(
            process_call(project, Some(session.session_id)),
            Some(&auth_context(None, true)),
        )
        .await;

    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "session_closed");
    assert_eq!(result.output["execution_state"], "not_started");
    assert_eq!(result.output["command_started"], false);
    assert_eq!(result.output["command_completed"], false);
    assert_eq!(result.output["failure_kind"], "session_closed");
    assert!(next_patch_agent_request(&runtime, "process-closed")
        .await
        .is_none());
}

#[tokio::test]
async fn model_facing_session_denials_keep_run_process_prestart_lifecycle() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project =
        register_process_agent(&runtime, "process-kernel-guards", temp.path(), true, false).await;
    let read_only = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        Some("read-only model process".to_string()),
        SessionMode::ReadOnly,
        sessions::SessionGuards::default(),
    );
    let closed = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        Some("closed model process".to_string()),
        SessionMode::Normal,
        sessions::SessionGuards::default(),
    );
    runtime.sessions.close_session(&closed.session_id).unwrap();
    let auth = auth_context(None, true);

    for (session_id, failure_kind) in [
        (read_only.session_id.as_str(), "session_guard_denied"),
        (closed.session_id.as_str(), "session_closed"),
    ] {
        let outcome = runtime
            .call_tool_with_context(
                ToolCallRequest {
                    tool_name: "run_process".to_string(),
                    arguments: json!({
                        "project": project.clone(),
                        "executable": "argv-helper",
                        "args": []
                    }),
                },
                ToolCallContext {
                    transport: ToolTransport::Api,
                    session_id: Some(session_id),
                    auth: Some(&auth),
                    window: None,
                    record_oauth_scope_denials: true,
                },
            )
            .await;
        let result = outcome.result.expect("structured Session denial");
        assert!(!result.success);
        assert_eq!(result.output["execution_state"], "not_started");
        assert_eq!(result.output["command_started"], false);
        assert_eq!(result.output["command_completed"], false);
        assert_eq!(result.output["failure_kind"], failure_kind);
    }
    assert!(
        next_patch_agent_request(&runtime, "process-kernel-guards")
            .await
            .is_none(),
        "model-facing Session denials must happen before Runner enqueue"
    );
}

//! Model-facing structured process execution contract.

use super::super::*;
use super::support::*;
use crate::shell_protocol::{
    ShellAgentResultPayload, ShellAgentResultRequest, ShellClientCapabilities,
    ShellCommandExecutionState,
};
use crate::tool_runtime::kernel::{ToolCallContext, ToolCallRequest, ToolTransport};
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

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
            timeout_secs: Some(121),
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
